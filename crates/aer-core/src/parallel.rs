//! Bounded parallel execution coordination and integration acceptance.
//!
//! Scheduling policy stays in `aer-domain`; resource/lease/cancellation truth
//! stays in `RuntimeSafetyKernel`; Git mutations stay in `aer-workspace`. This
//! module coordinates those authorities and prevents branch-local success from
//! being mistaken for integrated acceptance.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::{Component, Path},
};

use aer_domain::{
    cancellation::CancellationPhase,
    leases::LeasePolicy,
    resource_governor::{ResourceLimits, ResourceVector},
    runtime_safety::{AttemptOwnership, RuntimeSafetyError, RuntimeSafetyKernel},
    scheduling::{
        BoundedScheduler, PreemptionSafety, RuntimeWriteConflict, SchedulableTask, ScheduleWave,
        SchedulerPolicy, SemanticWriteSet, TaskGraph, WriteScope,
    },
    state_machines::{TaskState, TaskTransitionContext},
};
use aer_workspace::parallel::TaskChangeSet;

#[derive(Debug)]
pub struct ParallelRuntimeCoordinator {
    scheduler: BoundedScheduler,
    runtime: RuntimeSafetyKernel,
}

impl ParallelRuntimeCoordinator {
    #[must_use]
    pub fn new(
        scheduler_policy: SchedulerPolicy,
        resource_limits: ResourceLimits,
        lease_policy: LeasePolicy,
    ) -> Self {
        Self {
            scheduler: BoundedScheduler::new(scheduler_policy),
            runtime: RuntimeSafetyKernel::new(resource_limits, lease_policy),
        }
    }

    pub fn set_run_weight(
        &mut self,
        run_id: impl Into<String>,
        weight: u32,
    ) -> Result<(), ParallelExecutionError> {
        self.scheduler.set_run_weight(run_id, weight)?;
        Ok(())
    }

    /// Registers only schedulable lifecycle roots. Accepted/history nodes may
    /// remain in `TaskGraph` as dependency facts without being re-created as
    /// fresh runtime attempts.
    pub fn register_task(
        &mut self,
        task: &SchedulableTask,
        spec_version: u32,
    ) -> Result<(), ParallelExecutionError> {
        self.runtime.add_task(&task.task_id, spec_version)?;
        match task.state {
            TaskState::Pending => Ok(()),
            TaskState::Ready => {
                self.runtime.transition_task(
                    &task.task_id,
                    TaskState::Ready,
                    TaskTransitionContext::default(),
                )?;
                Ok(())
            }
            state => Err(ParallelExecutionError::UnsupportedRegistrationState {
                task_id: task.task_id.clone(),
                state,
            }),
        }
    }

    pub fn mark_ready(&mut self, task_id: &str) -> Result<(), ParallelExecutionError> {
        self.runtime.transition_task(
            task_id,
            TaskState::Ready,
            TaskTransitionContext::default(),
        )?;
        Ok(())
    }

    pub fn plan_wave(&self, graph: &TaskGraph) -> Result<ScheduleWave, ParallelExecutionError> {
        Ok(self.scheduler.plan_wave(graph)?)
    }

    /// Atomically coordinates scheduler bookkeeping with authoritative
    /// resource+lease admission. A denied resource reservation never leaves a
    /// ghost active scheduler record.
    pub fn admit_task(
        &mut self,
        task: &SchedulableTask,
        owner: impl Into<String>,
        now_ms: u64,
    ) -> Result<AttemptOwnership, ParallelExecutionError> {
        self.scheduler.record_admission(task)?;
        match self.runtime.start_task(
            &task.task_id,
            owner,
            task.admission_class,
            task.resource_estimate,
            task.effect_class,
            now_ms,
        ) {
            Ok(ownership) => Ok(ownership),
            Err(error) => {
                let rollback = self.scheduler.complete(&task.task_id);
                if rollback.is_err() {
                    return Err(ParallelExecutionError::CoordinatorInvariant(
                        "runtime admission failed and scheduler rollback failed",
                    ));
                }
                Err(error.into())
            }
        }
    }

    pub fn begin_verification(&mut self, task_id: &str) -> Result<(), ParallelExecutionError> {
        self.runtime.transition_task(
            task_id,
            TaskState::Verifying,
            TaskTransitionContext::default(),
        )?;
        Ok(())
    }

    pub fn finalize_verification(
        &mut self,
        task_id: &str,
        proof_accepted: bool,
    ) -> Result<TaskState, ParallelExecutionError> {
        let state = self
            .runtime
            .finalize_verification(task_id, proof_accepted)?;
        self.scheduler.complete(task_id).map_err(|_| {
            ParallelExecutionError::CoordinatorInvariant(
                "runtime verification finalized without active scheduler ownership",
            )
        })?;
        Ok(state)
    }

    pub fn observe_actual_write_scope(
        &mut self,
        task_id: &str,
        actual: WriteScope,
    ) -> Result<Vec<RuntimeWriteConflict>, ParallelExecutionError> {
        Ok(self.scheduler.observe_actual_write_scope(task_id, actual)?)
    }

    pub fn request_cancellation(
        &mut self,
        task_id: &str,
        now_ms: u64,
        cleanup_grace_ms: u64,
    ) -> Result<CancellationPhase, ParallelExecutionError> {
        Ok(self
            .runtime
            .request_cancellation(task_id, now_ms, cleanup_grace_ms)?)
    }

    /// Completes cancellation only after the caller confirms child/tool/process
    /// cleanup is drained. Runtime resource+lease release happens before
    /// scheduler ownership is removed.
    pub fn complete_cancellation(&mut self, task_id: &str) -> Result<(), ParallelExecutionError> {
        self.runtime.begin_cancellation_cleanup(task_id)?;
        self.runtime.complete_cancellation(task_id)?;
        self.scheduler.complete(task_id).map_err(|_| {
            ParallelExecutionError::CoordinatorInvariant(
                "runtime cancellation finalized without active scheduler ownership",
            )
        })?;
        Ok(())
    }

    /// Requests preemption only for a scheduler-proven safe candidate. External
    /// mutation and `Never` preemption tasks are excluded by domain policy.
    pub fn request_preemption_for(
        &mut self,
        incoming: &SchedulableTask,
        now_ms: u64,
        cleanup_grace_ms: u64,
    ) -> Result<Option<PreemptionRecord>, ParallelExecutionError> {
        let Some(candidate) = self.scheduler.preemption_candidate(incoming).cloned() else {
            return Ok(None);
        };
        let phase =
            self.runtime
                .request_cancellation(&candidate.task_id, now_ms, cleanup_grace_ms)?;
        Ok(Some(PreemptionRecord {
            preempted_task_id: candidate.task_id,
            incoming_task_id: incoming.task_id.clone(),
            safety: candidate.preemption,
            phase,
        }))
    }

    #[must_use]
    pub const fn resource_usage(&self) -> ResourceVector {
        self.runtime.resource_usage()
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.scheduler.active_count()
    }

    #[must_use]
    pub fn runtime_task(&self, task_id: &str) -> Option<&aer_domain::runtime_safety::RuntimeTask> {
        self.runtime.task(task_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreemptionRecord {
    pub preempted_task_id: String,
    pub incoming_task_id: String,
    pub safety: PreemptionSafety,
    pub phase: CancellationPhase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalBranchEvidence {
    pub task_id: String,
    pub changes: TaskChangeSet,
    pub semantic_write_set: SemanticWriteSet,
    pub local_verification_passed: bool,
    pub evidence_ids: BTreeSet<String>,
}

impl LocalBranchEvidence {
    pub fn validate(&self) -> Result<(), IntegrationError> {
        if self.task_id.trim().is_empty() || self.changes.branch_name.trim().is_empty() {
            return Err(IntegrationError::EmptyIdentity);
        }
        if self.changes.dirty {
            return Err(IntegrationError::DirtyBranch(self.task_id.clone()));
        }
        if self.changes.head_commit == self.changes.base_commit
            || self.changes.changed_paths.is_empty()
        {
            return Err(IntegrationError::EmptyChange(self.task_id.clone()));
        }
        if !self.local_verification_passed {
            return Err(IntegrationError::LocalVerificationFailed(
                self.task_id.clone(),
            ));
        }
        if self.evidence_ids.is_empty() {
            return Err(IntegrationError::MissingLocalEvidence(self.task_id.clone()));
        }
        for path in &self.changes.changed_paths {
            validate_repo_relative_path(path)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrationConflict {
    TextualPath {
        left_task: String,
        right_task: String,
        path: String,
    },
    Semantic {
        left_task: String,
        right_task: String,
        key: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationPlan {
    pub base_commit: String,
    pub ordered_tasks: Vec<String>,
    pub candidates: BTreeMap<String, LocalBranchEvidence>,
}

impl IntegrationPlan {
    pub fn build(
        candidates: Vec<LocalBranchEvidence>,
        max_candidates: usize,
    ) -> Result<Self, IntegrationError> {
        if max_candidates == 0 {
            return Err(IntegrationError::InvalidCandidateLimit);
        }
        if candidates.is_empty() {
            return Err(IntegrationError::NoCandidates);
        }
        if candidates.len() > max_candidates {
            return Err(IntegrationError::CandidateLimitExceeded {
                observed: candidates.len(),
                limit: max_candidates,
            });
        }

        let mut by_id = BTreeMap::new();
        let mut base_commit = None::<String>;
        for candidate in candidates {
            candidate.validate()?;
            if let Some(expected) = base_commit.as_ref() {
                if expected != &candidate.changes.base_commit {
                    return Err(IntegrationError::MixedBaseCommits {
                        expected: expected.clone(),
                        observed: candidate.changes.base_commit.clone(),
                    });
                }
            } else {
                base_commit = Some(candidate.changes.base_commit.clone());
            }
            if by_id.insert(candidate.task_id.clone(), candidate).is_some() {
                return Err(IntegrationError::DuplicateTaskEvidence);
            }
        }

        let values = by_id.values().collect::<Vec<_>>();
        for (index, left) in values.iter().enumerate() {
            for right in values.iter().skip(index + 1) {
                if let Some(conflict) = branch_conflict(left, right) {
                    return Err(IntegrationError::Conflict(conflict));
                }
            }
        }
        let ordered_tasks = by_id.keys().cloned().collect::<Vec<_>>();
        Ok(Self {
            base_commit: base_commit.expect("non-empty candidate set has a base"),
            ordered_tasks,
            candidates: by_id,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationBarrier {
    plan: IntegrationPlan,
    merged_heads: BTreeMap<String, String>,
}

impl IntegrationBarrier {
    #[must_use]
    pub fn new(plan: IntegrationPlan) -> Self {
        Self {
            plan,
            merged_heads: BTreeMap::new(),
        }
    }

    pub fn record_merge(
        &mut self,
        task_id: &str,
        resulting_head: impl Into<String>,
    ) -> Result<(), IntegrationError> {
        let expected = self
            .plan
            .ordered_tasks
            .get(self.merged_heads.len())
            .ok_or(IntegrationError::AllBranchesAlreadyMerged)?;
        if expected != task_id {
            return Err(IntegrationError::MergeOrderViolation {
                expected: expected.clone(),
                observed: task_id.to_owned(),
            });
        }
        let resulting_head = resulting_head.into();
        if resulting_head.trim().is_empty() {
            return Err(IntegrationError::EmptyIntegrationHead);
        }
        self.merged_heads.insert(task_id.to_owned(), resulting_head);
        Ok(())
    }

    #[must_use]
    pub fn ready_for_integration_verification(&self) -> bool {
        self.merged_heads.len() == self.plan.ordered_tasks.len()
    }

    #[must_use]
    pub fn current_head(&self) -> Option<&str> {
        self.plan
            .ordered_tasks
            .last()
            .and_then(|task_id| self.merged_heads.get(task_id))
            .map(String::as_str)
    }

    pub fn finalize(
        &self,
        verification: IntegrationVerification,
    ) -> Result<IntegratedAcceptance, IntegrationError> {
        if !self.ready_for_integration_verification() {
            return Err(IntegrationError::BranchesNotFullyMerged);
        }
        let expected_head = self
            .current_head()
            .ok_or(IntegrationError::BranchesNotFullyMerged)?;
        if verification.integration_head != expected_head {
            return Err(IntegrationError::VerificationHeadMismatch {
                expected: expected_head.to_owned(),
                observed: verification.integration_head,
            });
        }
        if !verification.passed {
            return Err(IntegrationError::IntegrationVerificationFailed);
        }
        if verification.proof_manifest_ids.is_empty()
            || verification.environment_fingerprint.trim().is_empty()
            || verification.repo_snapshot.trim().is_empty()
        {
            return Err(IntegrationError::MissingIntegrationProof);
        }
        Ok(IntegratedAcceptance {
            integration_head: expected_head.to_owned(),
            task_ids: self.plan.ordered_tasks.clone(),
            proof_manifest_ids: verification.proof_manifest_ids,
            environment_fingerprint: verification.environment_fingerprint,
            repo_snapshot: verification.repo_snapshot,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationVerification {
    pub passed: bool,
    pub integration_head: String,
    pub proof_manifest_ids: BTreeSet<String>,
    pub environment_fingerprint: String,
    pub repo_snapshot: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegratedAcceptance {
    pub integration_head: String,
    pub task_ids: Vec<String>,
    pub proof_manifest_ids: BTreeSet<String>,
    pub environment_fingerprint: String,
    pub repo_snapshot: String,
}

fn branch_conflict(
    left: &LocalBranchEvidence,
    right: &LocalBranchEvidence,
) -> Option<IntegrationConflict> {
    let left_paths = left
        .changes
        .changed_paths
        .iter()
        .map(|path| normalized_path(path))
        .collect::<BTreeSet<_>>();
    let right_paths = right
        .changes
        .changed_paths
        .iter()
        .map(|path| normalized_path(path))
        .collect::<BTreeSet<_>>();
    if let Some(path) = left_paths.intersection(&right_paths).next() {
        return Some(IntegrationConflict::TextualPath {
            left_task: left.task_id.clone(),
            right_task: right.task_id.clone(),
            path: path.clone(),
        });
    }
    for key in left.semantic_write_set.keys() {
        if right.semantic_write_set.keys().any(|other| other == key) {
            return Some(IntegrationConflict::Semantic {
                left_task: left.task_id.clone(),
                right_task: right.task_id.clone(),
                key: key.to_owned(),
            });
        }
    }
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParallelUtilityPolicy {
    pub min_time_saving_permille: u16,
    pub max_extra_cost_microusd: u64,
}

impl ParallelUtilityPolicy {
    pub fn new(
        min_time_saving_permille: u16,
        max_extra_cost_microusd: u64,
    ) -> Result<Self, ParallelUtilityError> {
        if min_time_saving_permille > 1000 {
            return Err(ParallelUtilityError::InvalidPolicy);
        }
        Ok(Self {
            min_time_saving_permille,
            max_extra_cost_microusd,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParallelUtilityMeasurement {
    pub serial_wall_ms: u64,
    pub parallel_wall_ms: u64,
    pub coordination_ms: u64,
    pub serial_verified_successes: u32,
    pub parallel_verified_successes: u32,
    pub serial_cost_microusd: u64,
    pub parallel_cost_microusd: u64,
}

impl ParallelUtilityMeasurement {
    pub fn positive(self, policy: ParallelUtilityPolicy) -> Result<bool, ParallelUtilityError> {
        if self.serial_wall_ms == 0 {
            return Err(ParallelUtilityError::InvalidMeasurement);
        }
        let effective_parallel = self
            .parallel_wall_ms
            .checked_add(self.coordination_ms)
            .ok_or(ParallelUtilityError::ArithmeticOverflow)?;
        let extra_cost = self
            .parallel_cost_microusd
            .saturating_sub(self.serial_cost_microusd);
        if extra_cost > policy.max_extra_cost_microusd {
            return Ok(false);
        }
        if self.parallel_verified_successes > self.serial_verified_successes {
            return Ok(true);
        }
        if self.parallel_verified_successes < self.serial_verified_successes
            || effective_parallel >= self.serial_wall_ms
        {
            return Ok(false);
        }
        let saved = self.serial_wall_ms - effective_parallel;
        let saving_permille = u128::from(saved)
            .checked_mul(1000)
            .ok_or(ParallelUtilityError::ArithmeticOverflow)?
            / u128::from(self.serial_wall_ms);
        Ok(saving_permille >= u128::from(policy.min_time_saving_permille))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OwnedResourceKind {
    Worktree,
    ProcessTree,
    EphemeralService,
    LocalPort,
    Container,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedRuntimeResource {
    pub resource_id: String,
    pub task_id: String,
    pub kind: OwnedResourceKind,
    pub cleanup_deadline_ms: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OrphanSweep {
    pub cleaned: Vec<String>,
    pub failed: Vec<String>,
    pub deferred: Vec<String>,
}

pub struct OrphanRegistry {
    max_records: usize,
    max_cleanup_per_sweep: usize,
    records: BTreeMap<String, OwnedRuntimeResource>,
}

impl OrphanRegistry {
    pub fn new(max_records: usize, max_cleanup_per_sweep: usize) -> Result<Self, OrphanError> {
        if max_records == 0 || max_cleanup_per_sweep == 0 || max_cleanup_per_sweep > max_records {
            return Err(OrphanError::InvalidPolicy);
        }
        Ok(Self {
            max_records,
            max_cleanup_per_sweep,
            records: BTreeMap::new(),
        })
    }

    pub fn register(&mut self, resource: OwnedRuntimeResource) -> Result<(), OrphanError> {
        if resource.resource_id.trim().is_empty() || resource.task_id.trim().is_empty() {
            return Err(OrphanError::EmptyIdentity);
        }
        if !self.records.contains_key(&resource.resource_id)
            && self.records.len() >= self.max_records
        {
            return Err(OrphanError::RegistryFull);
        }
        if self
            .records
            .insert(resource.resource_id.clone(), resource)
            .is_some()
        {
            return Err(OrphanError::DuplicateResource);
        }
        Ok(())
    }

    pub fn release(&mut self, resource_id: &str) -> Result<OwnedRuntimeResource, OrphanError> {
        self.records
            .remove(resource_id)
            .ok_or_else(|| OrphanError::UnknownResource(resource_id.to_owned()))
    }

    /// Cleans only resources whose owner task has no live durable ownership and
    /// whose cleanup deadline has arrived. Failed cleanup remains registered for
    /// retry/recovery; the registry never forgets an orphan merely because a
    /// cleanup callback failed.
    pub fn reconcile<F>(
        &mut self,
        live_task_ids: &BTreeSet<String>,
        now_ms: u64,
        mut cleanup: F,
    ) -> OrphanSweep
    where
        F: FnMut(&OwnedRuntimeResource) -> bool,
    {
        let eligible = self
            .records
            .values()
            .filter(|record| {
                !live_task_ids.contains(&record.task_id) && now_ms >= record.cleanup_deadline_ms
            })
            .map(|record| record.resource_id.clone())
            .collect::<Vec<_>>();
        let mut sweep = OrphanSweep::default();
        for (index, resource_id) in eligible.into_iter().enumerate() {
            if index >= self.max_cleanup_per_sweep {
                sweep.deferred.push(resource_id);
                continue;
            }
            let record = self
                .records
                .get(&resource_id)
                .expect("eligible resource comes from registry")
                .clone();
            if cleanup(&record) {
                self.records.remove(&resource_id);
                sweep.cleaned.push(resource_id);
            } else {
                sweep.failed.push(resource_id);
            }
        }
        sweep
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

fn validate_repo_relative_path(path: &Path) -> Result<(), IntegrationError> {
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(IntegrationError::UnsafeChangedPath(
            path.to_string_lossy().into_owned(),
        ));
    }
    Ok(())
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[derive(Debug)]
pub enum ParallelExecutionError {
    Runtime(RuntimeSafetyError),
    Scheduling(aer_domain::scheduling::SchedulingError),
    UnsupportedRegistrationState { task_id: String, state: TaskState },
    CoordinatorInvariant(&'static str),
}

impl fmt::Display for ParallelExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => error.fmt(formatter),
            Self::Scheduling(error) => error.fmt(formatter),
            Self::UnsupportedRegistrationState { task_id, state } => write!(
                formatter,
                "task {task_id} cannot be registered as fresh runtime work from state {state:?}"
            ),
            Self::CoordinatorInvariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for ParallelExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            Self::Scheduling(error) => Some(error),
            _ => None,
        }
    }
}

impl From<RuntimeSafetyError> for ParallelExecutionError {
    fn from(value: RuntimeSafetyError) -> Self {
        Self::Runtime(value)
    }
}

impl From<aer_domain::scheduling::SchedulingError> for ParallelExecutionError {
    fn from(value: aer_domain::scheduling::SchedulingError) -> Self {
        Self::Scheduling(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IntegrationError {
    EmptyIdentity,
    DirtyBranch(String),
    EmptyChange(String),
    LocalVerificationFailed(String),
    MissingLocalEvidence(String),
    UnsafeChangedPath(String),
    InvalidCandidateLimit,
    NoCandidates,
    CandidateLimitExceeded { observed: usize, limit: usize },
    MixedBaseCommits { expected: String, observed: String },
    DuplicateTaskEvidence,
    Conflict(IntegrationConflict),
    AllBranchesAlreadyMerged,
    MergeOrderViolation { expected: String, observed: String },
    EmptyIntegrationHead,
    BranchesNotFullyMerged,
    VerificationHeadMismatch { expected: String, observed: String },
    IntegrationVerificationFailed,
    MissingIntegrationProof,
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "integration error: {self:?}")
    }
}

impl Error for IntegrationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelUtilityError {
    InvalidPolicy,
    InvalidMeasurement,
    ArithmeticOverflow,
}

impl fmt::Display for ParallelUtilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "parallel utility error: {self:?}")
    }
}

impl Error for ParallelUtilityError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrphanError {
    InvalidPolicy,
    EmptyIdentity,
    RegistryFull,
    DuplicateResource,
    UnknownResource(String),
}

impl fmt::Display for OrphanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "orphan registry error: {self:?}")
    }
}

impl Error for OrphanError {}
