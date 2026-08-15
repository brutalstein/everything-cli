//! Single-coordinator runtime safety kernel.
//!
//! This module is intentionally deterministic and provider-independent. It
//! owns lifecycle/resource invariants; the future `aer-core` application layer
//! will journal successful commands before exposing them outside the control
//! plane.

use std::{collections::BTreeMap, error::Error, fmt};

use crate::{
    cancellation::{CancellationError, CancellationPhase, CancellationProtocol},
    leases::{EffectClass, LeaseBook, LeaseError, LeaseHealth, LeasePolicy},
    resource_governor::{
        AdmissionClass, ResourceError, ResourceEstimate, ResourceGovernor, ResourceLimits,
        ResourceVector,
    },
    state_machines::{
        ProjectRuntimeState, RunState, TaskState, TaskTransitionContext, TransitionError,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttemptOwnership {
    pub lease_id: u64,
    pub reservation_id: Option<u64>,
    pub effect_class: EffectClass,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeTask {
    pub state: TaskState,
    pub spec_version: u32,
    pub cancellation: CancellationProtocol,
    pub active_attempt: Option<AttemptOwnership>,
    pub reconciliation_required: bool,
}

pub struct RuntimeSafetyKernel {
    project_state: ProjectRuntimeState,
    runs: BTreeMap<String, RunState>,
    tasks: BTreeMap<String, RuntimeTask>,
    leases: LeaseBook,
    governor: ResourceGovernor,
}

impl RuntimeSafetyKernel {
    #[must_use]
    pub fn new(resource_limits: ResourceLimits, lease_policy: LeasePolicy) -> Self {
        Self {
            project_state: ProjectRuntimeState::Active,
            runs: BTreeMap::new(),
            tasks: BTreeMap::new(),
            leases: LeaseBook::new(lease_policy),
            governor: ResourceGovernor::new(resource_limits),
        }
    }

    pub fn transition_project(
        &mut self,
        next: ProjectRuntimeState,
    ) -> Result<ProjectRuntimeState, RuntimeSafetyError> {
        self.project_state = self.project_state.transition(next)?;
        Ok(self.project_state)
    }

    pub fn add_run(&mut self, run_id: impl Into<String>) -> Result<(), RuntimeSafetyError> {
        let run_id = run_id.into();
        if run_id.trim().is_empty() {
            return Err(RuntimeSafetyError::EmptyIdentity);
        }
        if self
            .runs
            .insert(run_id.clone(), RunState::Pending)
            .is_some()
        {
            return Err(RuntimeSafetyError::DuplicateRun(run_id));
        }
        Ok(())
    }

    pub fn transition_run(
        &mut self,
        run_id: &str,
        next: RunState,
    ) -> Result<RunState, RuntimeSafetyError> {
        let current = *self
            .runs
            .get(run_id)
            .ok_or_else(|| RuntimeSafetyError::UnknownRun(run_id.to_owned()))?;
        let next = current.transition(next)?;
        self.runs.insert(run_id.to_owned(), next);
        Ok(next)
    }

    pub fn add_task(
        &mut self,
        task_id: impl Into<String>,
        spec_version: u32,
    ) -> Result<(), RuntimeSafetyError> {
        let task_id = task_id.into();
        if task_id.trim().is_empty() || spec_version == 0 {
            return Err(RuntimeSafetyError::EmptyIdentity);
        }
        if self.tasks.contains_key(&task_id) {
            return Err(RuntimeSafetyError::DuplicateTask(task_id));
        }
        self.tasks.insert(
            task_id,
            RuntimeTask {
                state: TaskState::Pending,
                spec_version,
                cancellation: CancellationProtocol::default(),
                active_attempt: None,
                reconciliation_required: false,
            },
        );
        Ok(())
    }

    /// Performs non-terminal lifecycle transitions. Verification completion and
    /// cancellation terminalization are deliberately separate protocols because
    /// they must reconcile owned leases/resources before exposing the new state.
    pub fn transition_task(
        &mut self,
        task_id: &str,
        next: TaskState,
        context: TaskTransitionContext,
    ) -> Result<TaskState, RuntimeSafetyError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| RuntimeSafetyError::UnknownTask(task_id.to_owned()))?;
        if matches!(next, TaskState::Accepted | TaskState::Rejected | TaskState::Cancelled) {
            return Err(RuntimeSafetyError::FinalizationProtocolRequired(next));
        }
        if !task.cancellation.allows_new_child_actions() {
            return Err(RuntimeSafetyError::CancellationInProgress);
        }
        task.state = task.state.transition(next, context)?;
        Ok(task.state)
    }

    pub fn start_task(
        &mut self,
        task_id: &str,
        owner: impl Into<String>,
        class: AdmissionClass,
        estimate: ResourceEstimate,
        effect_class: EffectClass,
        now_ms: u64,
    ) -> Result<AttemptOwnership, RuntimeSafetyError> {
        if !self.project_state.admits_new_work() {
            return Err(RuntimeSafetyError::ProjectNotAdmittingWork);
        }
        let task = self
            .tasks
            .get(task_id)
            .ok_or_else(|| RuntimeSafetyError::UnknownTask(task_id.to_owned()))?;
        if task.state != TaskState::Ready {
            return Err(RuntimeSafetyError::TaskNotReady(task.state));
        }
        if task.reconciliation_required || task.active_attempt.is_some() {
            return Err(RuntimeSafetyError::AttemptAlreadyOwned);
        }
        if !task.cancellation.allows_new_child_actions() {
            return Err(RuntimeSafetyError::CancellationInProgress);
        }
        let next_state = task
            .state
            .transition(TaskState::Running, TaskTransitionContext::default())?;

        let owner = owner.into();
        let reservation = self.governor.admit(task_id.to_owned(), class, estimate)?;
        let lease = match self
            .leases
            .acquire(task_id.to_owned(), owner, effect_class, now_ms)
        {
            Ok(lease) => lease,
            Err(error) => {
                self.governor.release(reservation.id)?;
                return Err(error.into());
            }
        };
        let ownership = AttemptOwnership {
            lease_id: lease.id,
            reservation_id: Some(reservation.id),
            effect_class,
        };
        let task = self
            .tasks
            .get_mut(task_id)
            .expect("task existence checked before admission");
        task.state = next_state;
        task.active_attempt = Some(ownership.clone());
        Ok(ownership)
    }

    pub fn heartbeat_task(
        &mut self,
        task_id: &str,
        now_ms: u64,
    ) -> Result<LeaseHealth, RuntimeSafetyError> {
        let ownership = self.ownership(task_id)?.clone();
        let lease = self.leases.heartbeat(task_id, ownership.lease_id, now_ms)?;
        Ok(lease.health(now_ms))
    }

    /// Converts an expired execution into a blocked, reconciliation-required
    /// task. Local capacity is released immediately, while the lease identity is
    /// retained until explicit reconciliation prevents silent duplicate effects.
    pub fn observe_expired_lease(
        &mut self,
        task_id: &str,
        now_ms: u64,
    ) -> Result<LeaseHealth, RuntimeSafetyError> {
        let ownership = self.ownership(task_id)?.clone();
        let lease = self
            .leases
            .observe_expiry(task_id, ownership.lease_id, now_ms)?;
        if let Some(reservation_id) = ownership.reservation_id {
            self.governor.release(reservation_id)?;
        }
        let task = self
            .tasks
            .get_mut(task_id)
            .expect("ownership implies task existence");
        if task.state == TaskState::Running {
            task.state = task
                .state
                .transition(TaskState::Blocked, TaskTransitionContext::default())?;
        }
        if let Some(active) = task.active_attempt.as_mut() {
            active.reservation_id = None;
        }
        task.reconciliation_required = true;
        Ok(lease.health(now_ms))
    }

    pub fn reconcile_expired_lease(
        &mut self,
        task_id: &str,
        now_ms: u64,
    ) -> Result<(), RuntimeSafetyError> {
        let ownership = self.ownership(task_id)?.clone();
        self.leases
            .reconcile_expired(task_id, ownership.lease_id, now_ms)?;
        let task = self
            .tasks
            .get_mut(task_id)
            .expect("ownership implies task existence");
        task.active_attempt = None;
        task.reconciliation_required = false;
        Ok(())
    }

    /// Completes the verification protocol, releases attempt ownership, and
    /// only then publishes either `Accepted` or `Rejected` in memory.
    pub fn finalize_verification(
        &mut self,
        task_id: &str,
        proof_accepted: bool,
    ) -> Result<TaskState, RuntimeSafetyError> {
        let (ownership, next_state) = {
            let task = self
                .tasks
                .get(task_id)
                .ok_or_else(|| RuntimeSafetyError::UnknownTask(task_id.to_owned()))?;
            if !task.cancellation.allows_new_child_actions() {
                return Err(RuntimeSafetyError::CancellationInProgress);
            }
            let ownership = task
                .active_attempt
                .clone()
                .ok_or(RuntimeSafetyError::NoActiveAttempt)?;
            let target = if proof_accepted {
                TaskState::Accepted
            } else {
                TaskState::Rejected
            };
            let next_state = task.state.transition(
                target,
                TaskTransitionContext {
                    proof_accepted,
                    cancellation_finalized: false,
                },
            )?;
            (ownership, next_state)
        };

        self.release_attempt(task_id, &ownership)?;
        let task = self.task_mut(task_id)?;
        task.state = next_state;
        task.active_attempt = None;
        task.reconciliation_required = false;
        Ok(next_state)
    }

    pub fn request_cancellation(
        &mut self,
        task_id: &str,
        now_ms: u64,
        cleanup_grace_ms: u64,
    ) -> Result<CancellationPhase, RuntimeSafetyError> {
        let task = self.task_mut(task_id)?;
        if task.state.is_terminal() {
            return Err(RuntimeSafetyError::TaskAlreadyTerminal(task.state));
        }
        task.cancellation.request(now_ms, cleanup_grace_ms)?;
        Ok(task.cancellation.phase())
    }

    pub fn begin_cancellation_cleanup(
        &mut self,
        task_id: &str,
    ) -> Result<CancellationPhase, RuntimeSafetyError> {
        let task = self.task_mut(task_id)?;
        task.cancellation.begin_draining()?;
        Ok(task.cancellation.phase())
    }

    pub fn poll_cancellation(
        &mut self,
        task_id: &str,
        now_ms: u64,
    ) -> Result<CancellationPhase, RuntimeSafetyError> {
        Ok(self.task_mut(task_id)?.cancellation.poll_deadline(now_ms)?)
    }

    pub fn complete_cancellation(&mut self, task_id: &str) -> Result<(), RuntimeSafetyError> {
        let (ownership, completed_cancellation, cancelled_state) = {
            let task = self
                .tasks
                .get(task_id)
                .ok_or_else(|| RuntimeSafetyError::UnknownTask(task_id.to_owned()))?;
            let mut completed_cancellation = task.cancellation;
            completed_cancellation.complete()?;
            let cancelled_state = task.state.transition(
                TaskState::Cancelled,
                TaskTransitionContext {
                    proof_accepted: false,
                    cancellation_finalized: true,
                },
            )?;
            (
                task.active_attempt.clone(),
                completed_cancellation,
                cancelled_state,
            )
        };

        if let Some(ownership) = ownership.as_ref() {
            self.release_attempt(task_id, ownership)?;
        }
        let task = self.task_mut(task_id)?;
        task.cancellation = completed_cancellation;
        task.state = cancelled_state;
        task.active_attempt = None;
        task.reconciliation_required = false;
        Ok(())
    }

    #[must_use]
    pub fn child_actions_allowed(&self, task_id: &str) -> Option<bool> {
        self.tasks
            .get(task_id)
            .map(|task| task.cancellation.allows_new_child_actions())
    }

    #[must_use]
    pub fn task(&self, task_id: &str) -> Option<&RuntimeTask> {
        self.tasks.get(task_id)
    }

    #[must_use]
    pub const fn resource_usage(&self) -> ResourceVector {
        self.governor.usage()
    }

    fn release_attempt(
        &mut self,
        task_id: &str,
        ownership: &AttemptOwnership,
    ) -> Result<(), RuntimeSafetyError> {
        let lease = self
            .leases
            .lease(task_id)
            .ok_or_else(|| LeaseError::UnknownTask(task_id.to_owned()))?;
        if lease.id != ownership.lease_id {
            return Err(LeaseError::LeaseMismatch.into());
        }
        if let Some(reservation_id) = ownership.reservation_id {
            let reservation = self
                .governor
                .reservation_for(task_id)
                .ok_or(ResourceError::AccountingInvariant)?;
            if reservation.id != reservation_id {
                return Err(ResourceError::AccountingInvariant.into());
            }
            self.governor.release(reservation_id)?;
        }
        self.leases.release(task_id, ownership.lease_id)?;
        Ok(())
    }

    fn ownership(&self, task_id: &str) -> Result<&AttemptOwnership, RuntimeSafetyError> {
        self.tasks
            .get(task_id)
            .ok_or_else(|| RuntimeSafetyError::UnknownTask(task_id.to_owned()))?
            .active_attempt
            .as_ref()
            .ok_or(RuntimeSafetyError::NoActiveAttempt)
    }

    fn task_mut(&mut self, task_id: &str) -> Result<&mut RuntimeTask, RuntimeSafetyError> {
        self.tasks
            .get_mut(task_id)
            .ok_or_else(|| RuntimeSafetyError::UnknownTask(task_id.to_owned()))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum RuntimeSafetyError {
    EmptyIdentity,
    DuplicateRun(String),
    DuplicateTask(String),
    UnknownRun(String),
    UnknownTask(String),
    ProjectNotAdmittingWork,
    TaskNotReady(TaskState),
    TaskAlreadyTerminal(TaskState),
    FinalizationProtocolRequired(TaskState),
    AttemptAlreadyOwned,
    NoActiveAttempt,
    CancellationInProgress,
    Transition(TransitionError),
    Resource(ResourceError),
    Lease(LeaseError),
    Cancellation(CancellationError),
}

impl fmt::Display for RuntimeSafetyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentity => {
                formatter.write_str("runtime identity/version must be non-empty/nonzero")
            }
            Self::DuplicateRun(id) => write!(formatter, "duplicate run: {id}"),
            Self::DuplicateTask(id) => write!(formatter, "duplicate task: {id}"),
            Self::UnknownRun(id) => write!(formatter, "unknown run: {id}"),
            Self::UnknownTask(id) => write!(formatter, "unknown task: {id}"),
            Self::ProjectNotAdmittingWork => {
                formatter.write_str("project is not admitting new work")
            }
            Self::TaskNotReady(state) => write!(formatter, "task is not ready: {state:?}"),
            Self::TaskAlreadyTerminal(state) => {
                write!(formatter, "task is already terminal: {state:?}")
            }
            Self::FinalizationProtocolRequired(state) => {
                write!(formatter, "task state {state:?} requires a finalization protocol")
            }
            Self::AttemptAlreadyOwned => {
                formatter.write_str("task already has owned attempt state")
            }
            Self::NoActiveAttempt => formatter.write_str("task has no active attempt"),
            Self::CancellationInProgress => {
                formatter.write_str("cancellation blocks new child actions")
            }
            Self::Transition(error) => error.fmt(formatter),
            Self::Resource(error) => error.fmt(formatter),
            Self::Lease(error) => error.fmt(formatter),
            Self::Cancellation(error) => error.fmt(formatter),
        }
    }
}

impl Error for RuntimeSafetyError {}

impl From<TransitionError> for RuntimeSafetyError {
    fn from(value: TransitionError) -> Self {
        Self::Transition(value)
    }
}

impl From<ResourceError> for RuntimeSafetyError {
    fn from(value: ResourceError) -> Self {
        Self::Resource(value)
    }
}

impl From<LeaseError> for RuntimeSafetyError {
    fn from(value: LeaseError) -> Self {
        Self::Lease(value)
    }
}

impl From<CancellationError> for RuntimeSafetyError {
    fn from(value: CancellationError) -> Self {
        Self::Cancellation(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        cancellation::CancellationPhase,
        leases::{EffectClass, LeasePolicy},
        resource_governor::{AdmissionClass, ResourceEstimate, ResourceLimits, ResourceVector},
        state_machines::{TaskState, TaskTransitionContext},
    };

    use super::{RuntimeSafetyError, RuntimeSafetyKernel};

    fn kernel() -> RuntimeSafetyKernel {
        let hard = ResourceVector {
            worker_slots: 2,
            memory_bytes: 100,
            pids: 8,
            ..ResourceVector::default()
        };
        RuntimeSafetyKernel::new(
            ResourceLimits::new(hard, 1).expect("limits"),
            LeasePolicy::new(10, 20).expect("lease policy"),
        )
    }

    fn ready_task(kernel: &mut RuntimeSafetyKernel, task_id: &str) {
        kernel.add_task(task_id, 1).expect("task");
        kernel
            .transition_task(task_id, TaskState::Ready, TaskTransitionContext::default())
            .expect("ready");
    }

    fn demand() -> ResourceEstimate {
        ResourceEstimate::Known(ResourceVector {
            worker_slots: 1,
            memory_bytes: 10,
            pids: 1,
            ..ResourceVector::default()
        })
    }

    #[test]
    fn cancellation_eventually_releases_lease_and_resources() {
        let mut kernel = kernel();
        ready_task(&mut kernel, "task");
        kernel
            .start_task(
                "task",
                "worker",
                AdmissionClass::Generator,
                demand(),
                EffectClass::WorkspaceLocal,
                0,
            )
            .expect("start");
        assert_eq!(kernel.resource_usage().worker_slots, 1);
        assert_eq!(
            kernel.request_cancellation("task", 1, 4),
            Ok(CancellationPhase::Requested)
        );
        assert_eq!(kernel.child_actions_allowed("task"), Some(false));
        kernel.begin_cancellation_cleanup("task").expect("draining");
        assert_eq!(
            kernel.poll_cancellation("task", 5),
            Ok(CancellationPhase::ForceRequired)
        );
        kernel.complete_cancellation("task").expect("complete");
        assert_eq!(kernel.resource_usage(), ResourceVector::default());
        let task = kernel.task("task").expect("task state");
        assert_eq!(task.state, TaskState::Cancelled);
        assert!(task.active_attempt.is_none());
    }

    #[test]
    fn generic_transition_cannot_bypass_finalization_protocols() {
        let mut kernel = kernel();
        ready_task(&mut kernel, "task");
        assert_eq!(
            kernel.transition_task(
                "task",
                TaskState::Cancelled,
                TaskTransitionContext {
                    proof_accepted: false,
                    cancellation_finalized: true,
                },
            ),
            Err(RuntimeSafetyError::FinalizationProtocolRequired(
                TaskState::Cancelled
            ))
        );
    }

    #[test]
    fn verification_finalization_releases_attempt_for_reject_and_accept() {
        let mut kernel = kernel();
        ready_task(&mut kernel, "task");
        kernel
            .start_task(
                "task",
                "worker-1",
                AdmissionClass::Generator,
                demand(),
                EffectClass::Pure,
                0,
            )
            .expect("first attempt");
        kernel
            .transition_task(
                "task",
                TaskState::Verifying,
                TaskTransitionContext::default(),
            )
            .expect("verifying");
        assert_eq!(
            kernel.finalize_verification("task", false),
            Ok(TaskState::Rejected)
        );
        assert_eq!(kernel.resource_usage(), ResourceVector::default());
        assert!(kernel.task("task").expect("task").active_attempt.is_none());

        kernel
            .transition_task("task", TaskState::Ready, TaskTransitionContext::default())
            .expect("retry ready");
        kernel
            .start_task(
                "task",
                "worker-2",
                AdmissionClass::Generator,
                demand(),
                EffectClass::Pure,
                1,
            )
            .expect("second attempt");
        kernel
            .transition_task(
                "task",
                TaskState::Verifying,
                TaskTransitionContext::default(),
            )
            .expect("verifying again");
        assert_eq!(
            kernel.finalize_verification("task", true),
            Ok(TaskState::Accepted)
        );
        assert_eq!(kernel.resource_usage(), ResourceVector::default());
        assert!(kernel.task("task").expect("task").active_attempt.is_none());
        assert_eq!(
            kernel.request_cancellation("task", 2, 1),
            Err(RuntimeSafetyError::TaskAlreadyTerminal(TaskState::Accepted))
        );
    }

    #[test]
    fn expired_external_attempt_blocks_retry_until_reconciled() {
        let mut kernel = kernel();
        ready_task(&mut kernel, "task");
        kernel
            .start_task(
                "task",
                "worker",
                AdmissionClass::Generator,
                demand(),
                EffectClass::ExternalMutating,
                0,
            )
            .expect("start");
        kernel.observe_expired_lease("task", 20).expect("expire");
        assert_eq!(kernel.resource_usage(), ResourceVector::default());
        assert!(kernel.task("task").expect("task").reconciliation_required);
        kernel
            .reconcile_expired_lease("task", 20)
            .expect("reconcile");
        kernel
            .transition_task("task", TaskState::Ready, TaskTransitionContext::default())
            .expect("recovery returns task to ready");
        kernel
            .start_task(
                "task",
                "worker-2",
                AdmissionClass::Generator,
                demand(),
                EffectClass::ExternalMutating,
                21,
            )
            .expect("retry after reconciliation");
    }

    #[test]
    fn verifier_reservation_survives_generator_saturation() {
        let mut kernel = kernel();
        ready_task(&mut kernel, "generator");
        ready_task(&mut kernel, "verifier");
        kernel
            .start_task(
                "generator",
                "worker-g",
                AdmissionClass::Generator,
                demand(),
                EffectClass::Pure,
                0,
            )
            .expect("generator");
        kernel
            .start_task(
                "verifier",
                "worker-v",
                AdmissionClass::Verifier,
                demand(),
                EffectClass::Pure,
                0,
            )
            .expect("verifier uses reserved slot");
        assert_eq!(kernel.resource_usage().worker_slots, 2);
    }
}
