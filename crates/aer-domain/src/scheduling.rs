//! Deterministic bounded scheduling primitives for parallel engineering work.
//!
//! This module deliberately owns policy and coordination facts only. It does
//! not spawn workers or mutate repositories. Resource admission remains owned
//! by `ResourceGovernor`; leases/cancellation remain owned by
//! `RuntimeSafetyKernel`; worktree mutation remains owned by `aer-workspace`.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use crate::{
    leases::EffectClass,
    resource_governor::{AdmissionClass, ResourceEstimate, ResourceVector},
    state_machines::{TaskState, TaskTransitionContext, TransitionError},
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TaskRisk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreemptionSafety {
    Never,
    Discardable,
    Checkpointable,
}

/// Repository-relative predicted or observed write scope.
///
/// Prefixes use slash-normalized repository paths. Component-aware overlap is
/// intentional: `src/auth` overlaps `src/auth/token.rs`, while `src/auth` does
/// not overlap `src/authz`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WriteScope {
    repository_wide: bool,
    prefixes: BTreeSet<String>,
}

impl WriteScope {
    #[must_use]
    pub fn read_only() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn repository_wide() -> Self {
        Self {
            repository_wide: true,
            prefixes: BTreeSet::new(),
        }
    }

    pub fn new<I, S>(prefixes: I) -> Result<Self, SchedulingError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut normalized = BTreeSet::new();
        for prefix in prefixes {
            normalized.insert(normalize_repo_path(prefix.as_ref())?);
        }
        Ok(Self {
            repository_wide: false,
            prefixes: normalized,
        })
    }

    #[must_use]
    pub fn is_read_only(&self) -> bool {
        !self.repository_wide && self.prefixes.is_empty()
    }

    #[must_use]
    pub fn is_repository_wide(&self) -> bool {
        self.repository_wide
    }

    pub fn contains_path(&self, path: &str) -> Result<bool, SchedulingError> {
        let path = normalize_repo_path(path)?;
        Ok(self.repository_wide
            || self
                .prefixes
                .iter()
                .any(|prefix| component_prefix(prefix, &path)))
    }

    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        if self.is_read_only() || other.is_read_only() {
            return false;
        }
        if self.repository_wide || other.repository_wide {
            return true;
        }
        self.prefixes.iter().any(|left| {
            other
                .prefixes
                .iter()
                .any(|right| component_prefix(left, right) || component_prefix(right, left))
        })
    }

    pub fn prefixes(&self) -> impl Iterator<Item = &str> {
        self.prefixes.iter().map(String::as_str)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticWriteSet {
    keys: BTreeSet<String>,
}

impl SemanticWriteSet {
    pub fn new<I, S>(keys: I) -> Result<Self, SchedulingError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut normalized = BTreeSet::new();
        for key in keys {
            let key = key.as_ref().trim();
            if key.is_empty() {
                return Err(SchedulingError::EmptySemanticKey);
            }
            normalized.insert(key.to_owned());
        }
        Ok(Self { keys: normalized })
    }

    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.keys.iter().any(|key| other.keys.contains(key))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.keys.iter().map(String::as_str)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrioritySignals {
    pub unblock_value: u32,
    pub critical_path_weight: u32,
    pub risk_reduction_value: u32,
    pub information_gain_value: u32,
    pub user_waiting_value: u32,
    pub age_value: u32,
    pub expected_cost_penalty: u32,
    pub merge_conflict_risk: u32,
}

impl PrioritySignals {
    #[must_use]
    pub fn score(self) -> i64 {
        let positive = i64::from(self.unblock_value)
            + i64::from(self.critical_path_weight)
            + i64::from(self.risk_reduction_value)
            + i64::from(self.information_gain_value)
            + i64::from(self.user_waiting_value)
            + i64::from(self.age_value);
        let negative = i64::from(self.expected_cost_penalty) + i64::from(self.merge_conflict_risk);
        positive - negative
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulableTask {
    pub task_id: String,
    pub run_id: String,
    pub state: TaskState,
    pub dependencies: BTreeSet<String>,
    pub admission_class: AdmissionClass,
    pub resource_estimate: ResourceEstimate,
    pub effect_class: EffectClass,
    pub expected_write_scope: WriteScope,
    pub semantic_write_set: SemanticWriteSet,
    pub risk: TaskRisk,
    pub contracts_stable: bool,
    pub has_local_verifier: bool,
    pub serial_only: bool,
    pub preemption: PreemptionSafety,
    pub priority: PrioritySignals,
}

impl SchedulableTask {
    pub fn validate(&self) -> Result<(), SchedulingError> {
        if self.task_id.trim().is_empty() || self.run_id.trim().is_empty() {
            return Err(SchedulingError::EmptyTaskIdentity);
        }
        if self.dependencies.contains(&self.task_id) {
            return Err(SchedulingError::SelfDependency(self.task_id.clone()));
        }
        Ok(())
    }

    fn resource_bound(&self) -> Result<ResourceVector, SchedulingError> {
        match self.resource_estimate {
            ResourceEstimate::Known(vector) | ResourceEstimate::UpperBound(vector) => Ok(vector),
            ResourceEstimate::Unknown => {
                Err(SchedulingError::UnknownResourceDemand(self.task_id.clone()))
            }
        }
    }

    fn service_cost(&self) -> Result<u64, SchedulingError> {
        let demand = self.resource_bound()?;
        Ok(demand.worker_slots.max(demand.provider_concurrency).max(1))
    }
}

#[derive(Clone, Debug)]
pub struct TaskGraph {
    tasks: BTreeMap<String, SchedulableTask>,
}

impl TaskGraph {
    pub fn new(tasks: Vec<SchedulableTask>) -> Result<Self, SchedulingError> {
        let mut by_id = BTreeMap::new();
        for task in tasks {
            task.validate()?;
            let id = task.task_id.clone();
            if by_id.insert(id.clone(), task).is_some() {
                return Err(SchedulingError::DuplicateTask(id));
            }
        }
        for task in by_id.values() {
            for dependency in &task.dependencies {
                if !by_id.contains_key(dependency) {
                    return Err(SchedulingError::UnknownDependency {
                        task_id: task.task_id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }
        ensure_acyclic(&by_id)?;
        Ok(Self { tasks: by_id })
    }

    pub fn tasks(&self) -> impl Iterator<Item = &SchedulableTask> {
        self.tasks.values()
    }

    #[must_use]
    pub fn task(&self, task_id: &str) -> Option<&SchedulableTask> {
        self.tasks.get(task_id)
    }

    pub fn transition_task(
        &mut self,
        task_id: &str,
        next: TaskState,
        context: TaskTransitionContext,
    ) -> Result<TaskState, SchedulingError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| SchedulingError::UnknownTask(task_id.to_owned()))?;
        task.state = task.state.transition(next, context)?;
        Ok(task.state)
    }

    /// Promotes dependency-satisfied pending nodes to ready in deterministic ID
    /// order. The graph itself is durable state, not an execution queue.
    pub fn refresh_readiness(&mut self) -> Result<Vec<String>, SchedulingError> {
        let ready = self
            .tasks
            .values()
            .filter(|task| {
                task.state == TaskState::Pending
                    && task.dependencies.iter().all(|dependency| {
                        self.tasks
                            .get(dependency)
                            .is_some_and(|candidate| candidate.state == TaskState::Accepted)
                    })
            })
            .map(|task| task.task_id.clone())
            .collect::<Vec<_>>();
        for task_id in &ready {
            self.transition_task(task_id, TaskState::Ready, TaskTransitionContext::default())?;
        }
        Ok(ready)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerPolicy {
    pub max_parallel_tasks: usize,
    pub max_ready_tasks: usize,
    pub max_deferred_records: usize,
    pub serialize_high_risk: bool,
    pub require_stable_contracts: bool,
    pub require_local_verifier: bool,
}

impl SchedulerPolicy {
    pub fn new(max_parallel_tasks: usize, max_ready_tasks: usize) -> Result<Self, SchedulingError> {
        if max_parallel_tasks == 0 || max_ready_tasks == 0 || max_parallel_tasks > max_ready_tasks {
            return Err(SchedulingError::InvalidSchedulerPolicy);
        }
        Ok(Self {
            max_parallel_tasks,
            max_ready_tasks,
            max_deferred_records: max_ready_tasks,
            serialize_high_risk: true,
            require_stable_contracts: true,
            require_local_verifier: true,
        })
    }

    #[must_use]
    pub fn with_deferred_record_limit(mut self, limit: usize) -> Self {
        self.max_deferred_records = limit.max(1);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveTask {
    pub task_id: String,
    pub run_id: String,
    pub expected_write_scope: WriteScope,
    pub semantic_write_set: SemanticWriteSet,
    pub admission_class: AdmissionClass,
    pub effect_class: EffectClass,
    pub preemption: PreemptionSafety,
    pub risk: TaskRisk,
    pub serial_only: bool,
    pub priority_score: i64,
}

impl ActiveTask {
    fn from_task(task: &SchedulableTask) -> Self {
        Self {
            task_id: task.task_id.clone(),
            run_id: task.run_id.clone(),
            expected_write_scope: task.expected_write_scope.clone(),
            semantic_write_set: task.semantic_write_set.clone(),
            admission_class: task.admission_class,
            effect_class: task.effect_class,
            preemption: task.preemption,
            risk: task.risk,
            serial_only: task.serial_only,
            priority_score: task.priority.score(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduleBlockReason {
    UnstableContracts,
    MissingLocalVerifier,
    UnknownResourceDemand,
    SerialOnly,
    HighRiskSerialization,
    WaveCapacity,
    SerializationBarrier,
    PredictedWriteOverlap { with_task: String },
    SemanticOverlap { with_task: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleDecision {
    pub task_id: String,
    pub run_id: String,
    pub priority_score: i64,
    pub run_service_units_before: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeferredTask {
    pub task_id: String,
    pub reason: ScheduleBlockReason,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScheduleWave {
    pub selected: Vec<ScheduleDecision>,
    pub deferred: Vec<DeferredTask>,
    pub omitted_deferred_records: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeWriteConflict {
    pub task_id: String,
    pub with_task: String,
    pub predicted_overlap: bool,
}

pub struct BoundedScheduler {
    policy: SchedulerPolicy,
    run_weights: BTreeMap<String, u32>,
    run_service_units: BTreeMap<String, u64>,
    active: BTreeMap<String, ActiveTask>,
    actual_write_scopes: BTreeMap<String, WriteScope>,
}

impl BoundedScheduler {
    #[must_use]
    pub fn new(policy: SchedulerPolicy) -> Self {
        Self {
            policy,
            run_weights: BTreeMap::new(),
            run_service_units: BTreeMap::new(),
            active: BTreeMap::new(),
            actual_write_scopes: BTreeMap::new(),
        }
    }

    pub fn set_run_weight(
        &mut self,
        run_id: impl Into<String>,
        weight: u32,
    ) -> Result<(), SchedulingError> {
        let run_id = run_id.into();
        if run_id.trim().is_empty() || weight == 0 {
            return Err(SchedulingError::InvalidRunWeight);
        }
        self.run_weights.insert(run_id, weight);
        Ok(())
    }

    pub fn plan_wave(&self, graph: &TaskGraph) -> Result<ScheduleWave, SchedulingError> {
        let mut candidates = graph
            .tasks()
            .filter(|task| {
                task.state == TaskState::Ready && !self.active.contains_key(&task.task_id)
            })
            .collect::<Vec<_>>();
        if candidates.len() > self.policy.max_ready_tasks {
            return Err(SchedulingError::ReadySetExceeded {
                ready: candidates.len(),
                limit: self.policy.max_ready_tasks,
            });
        }

        let available = self
            .policy
            .max_parallel_tasks
            .saturating_sub(self.active.len());
        let mut wave = ScheduleWave::default();
        if available == 0 {
            return Ok(wave);
        }

        let mut service_shadow = self.run_service_units.clone();
        let mut selected_active = Vec::<ActiveTask>::new();
        let mut serialization_barrier = false;

        while !candidates.is_empty() && wave.selected.len() < available {
            let best_index = (0..candidates.len())
                .min_by(|left, right| {
                    candidate_cmp(
                        candidates[*left],
                        candidates[*right],
                        &service_shadow,
                        &self.run_weights,
                    )
                })
                .expect("non-empty candidate set");
            let task = candidates.remove(best_index);

            if let Some(reason) = self.block_reason(task, &selected_active) {
                push_deferred(&mut wave, self.policy.max_deferred_records, task, reason);
                continue;
            }

            let service_before = *service_shadow.get(&task.run_id).unwrap_or(&0);
            wave.selected.push(ScheduleDecision {
                task_id: task.task_id.clone(),
                run_id: task.run_id.clone(),
                priority_score: task.priority.score(),
                run_service_units_before: service_before,
            });
            selected_active.push(ActiveTask::from_task(task));
            let next = service_before
                .checked_add(task.service_cost()?)
                .ok_or(SchedulingError::ServiceAccountingOverflow)?;
            service_shadow.insert(task.run_id.clone(), next);

            if task.serial_only
                || (self.policy.serialize_high_risk
                    && matches!(task.risk, TaskRisk::High | TaskRisk::Critical))
            {
                serialization_barrier = true;
                break;
            }
        }

        let remaining_reason = if serialization_barrier {
            ScheduleBlockReason::SerializationBarrier
        } else {
            ScheduleBlockReason::WaveCapacity
        };
        for task in candidates {
            push_deferred(
                &mut wave,
                self.policy.max_deferred_records,
                task,
                remaining_reason.clone(),
            );
        }
        Ok(wave)
    }

    pub fn record_admission(
        &mut self,
        task: &SchedulableTask,
    ) -> Result<ActiveTask, SchedulingError> {
        if self.active.contains_key(&task.task_id) {
            return Err(SchedulingError::TaskAlreadyActive(task.task_id.clone()));
        }
        if self.active.len() >= self.policy.max_parallel_tasks {
            return Err(SchedulingError::ActiveTaskLimit);
        }
        if let Some(reason) = self.block_reason(task, &[]) {
            return Err(SchedulingError::AdmissionBlocked {
                task_id: task.task_id.clone(),
                reason,
            });
        }
        let service = *self.run_service_units.get(&task.run_id).unwrap_or(&0);
        let next = service
            .checked_add(task.service_cost()?)
            .ok_or(SchedulingError::ServiceAccountingOverflow)?;
        self.run_service_units.insert(task.run_id.clone(), next);
        let active = ActiveTask::from_task(task);
        self.active.insert(task.task_id.clone(), active.clone());
        Ok(active)
    }

    pub fn complete(&mut self, task_id: &str) -> Result<ActiveTask, SchedulingError> {
        self.actual_write_scopes.remove(task_id);
        self.active
            .remove(task_id)
            .ok_or_else(|| SchedulingError::TaskNotActive(task_id.to_owned()))
    }

    pub fn observe_actual_write_scope(
        &mut self,
        task_id: &str,
        actual: WriteScope,
    ) -> Result<Vec<RuntimeWriteConflict>, SchedulingError> {
        let active = self
            .active
            .get(task_id)
            .cloned()
            .ok_or_else(|| SchedulingError::TaskNotActive(task_id.to_owned()))?;
        let mut conflicts = Vec::new();
        for other in self.active.values() {
            if other.task_id == task_id {
                continue;
            }
            let other_scope = self
                .actual_write_scopes
                .get(&other.task_id)
                .unwrap_or(&other.expected_write_scope);
            if actual.overlaps(other_scope) {
                conflicts.push(RuntimeWriteConflict {
                    task_id: task_id.to_owned(),
                    with_task: other.task_id.clone(),
                    predicted_overlap: active
                        .expected_write_scope
                        .overlaps(&other.expected_write_scope),
                });
            }
        }
        self.actual_write_scopes.insert(task_id.to_owned(), actual);
        Ok(conflicts)
    }

    #[must_use]
    pub fn preemption_candidate(&self, incoming: &SchedulableTask) -> Option<&ActiveTask> {
        let incoming_score = incoming.priority.score();
        self.active
            .values()
            .filter(|active| {
                active.admission_class == AdmissionClass::Generator
                    && active.effect_class != EffectClass::ExternalMutating
                    && active.preemption != PreemptionSafety::Never
                    && active.priority_score < incoming_score
            })
            .min_by(|left, right| {
                left.priority_score
                    .cmp(&right.priority_score)
                    .then_with(|| left.task_id.cmp(&right.task_id))
            })
    }

    #[must_use]
    pub fn orphaned_task_ids(&self, live_lease_tasks: &BTreeSet<String>) -> Vec<String> {
        self.active
            .keys()
            .filter(|task_id| !live_lease_tasks.contains(*task_id))
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    #[must_use]
    pub fn run_service_units(&self, run_id: &str) -> u64 {
        *self.run_service_units.get(run_id).unwrap_or(&0)
    }

    fn block_reason(
        &self,
        task: &SchedulableTask,
        selected: &[ActiveTask],
    ) -> Option<ScheduleBlockReason> {
        if self.policy.require_stable_contracts && !task.contracts_stable {
            return Some(ScheduleBlockReason::UnstableContracts);
        }
        if self.policy.require_local_verifier && !task.has_local_verifier {
            return Some(ScheduleBlockReason::MissingLocalVerifier);
        }
        if matches!(task.resource_estimate, ResourceEstimate::Unknown) {
            return Some(ScheduleBlockReason::UnknownResourceDemand);
        }
        if self.active.values().chain(selected.iter()).any(|active| {
            active.serial_only
                || (self.policy.serialize_high_risk
                    && matches!(active.risk, TaskRisk::High | TaskRisk::Critical))
        }) {
            return Some(ScheduleBlockReason::SerializationBarrier);
        }
        if task.serial_only && (!self.active.is_empty() || !selected.is_empty()) {
            return Some(ScheduleBlockReason::SerialOnly);
        }
        if self.policy.serialize_high_risk
            && matches!(task.risk, TaskRisk::High | TaskRisk::Critical)
            && (!self.active.is_empty() || !selected.is_empty())
        {
            return Some(ScheduleBlockReason::HighRiskSerialization);
        }

        for other in self.active.values().chain(selected.iter()) {
            if task
                .expected_write_scope
                .overlaps(&other.expected_write_scope)
            {
                return Some(ScheduleBlockReason::PredictedWriteOverlap {
                    with_task: other.task_id.clone(),
                });
            }
            if task.semantic_write_set.overlaps(&other.semantic_write_set) {
                return Some(ScheduleBlockReason::SemanticOverlap {
                    with_task: other.task_id.clone(),
                });
            }
        }
        None
    }
}

fn candidate_cmp(
    left: &SchedulableTask,
    right: &SchedulableTask,
    service: &BTreeMap<String, u64>,
    weights: &BTreeMap<String, u32>,
) -> Ordering {
    let left_service = u128::from(*service.get(&left.run_id).unwrap_or(&0));
    let right_service = u128::from(*service.get(&right.run_id).unwrap_or(&0));
    let left_weight = u128::from(*weights.get(&left.run_id).unwrap_or(&1));
    let right_weight = u128::from(*weights.get(&right.run_id).unwrap_or(&1));

    // Compare service/weight without floating point. Lower normalized service is
    // favored; priority and stable task identity break ties deterministically.
    (left_service * right_weight)
        .cmp(&(right_service * left_weight))
        .then_with(|| right.priority.score().cmp(&left.priority.score()))
        .then_with(|| left.task_id.cmp(&right.task_id))
}

fn push_deferred(
    wave: &mut ScheduleWave,
    limit: usize,
    task: &SchedulableTask,
    reason: ScheduleBlockReason,
) {
    if wave.deferred.len() < limit {
        wave.deferred.push(DeferredTask {
            task_id: task.task_id.clone(),
            reason,
        });
    } else {
        wave.omitted_deferred_records = wave.omitted_deferred_records.saturating_add(1);
    }
}

fn ensure_acyclic(tasks: &BTreeMap<String, SchedulableTask>) -> Result<(), SchedulingError> {
    let mut indegree = tasks
        .iter()
        .map(|(id, task)| (id.clone(), task.dependencies.len()))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = BTreeMap::<String, Vec<String>>::new();
    for task in tasks.values() {
        for dependency in &task.dependencies {
            outgoing
                .entry(dependency.clone())
                .or_default()
                .push(task.task_id.clone());
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(id.clone()))
        .collect::<BTreeSet<_>>();
    let mut visited = 0_usize;
    while let Some(id) = ready.pop_first() {
        visited += 1;
        if let Some(children) = outgoing.get(&id) {
            for child in children {
                let degree = indegree
                    .get_mut(child)
                    .expect("child originates from validated task graph");
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(child.clone());
                }
            }
        }
    }
    if visited != tasks.len() {
        return Err(SchedulingError::DependencyCycle);
    }
    Ok(())
}

fn normalize_repo_path(raw: &str) -> Result<String, SchedulingError> {
    let raw = raw.trim().replace('\\', "/");
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.starts_with("//")
        || raw
            .split('/')
            .next()
            .is_some_and(|first| first.ends_with(':'))
    {
        return Err(SchedulingError::UnsafeWritePath(raw));
    }
    let mut parts = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(SchedulingError::UnsafeWritePath(raw)),
            value => parts.push(value),
        }
    }
    if parts.is_empty() {
        return Err(SchedulingError::UnsafeWritePath(raw));
    }
    Ok(parts.join("/"))
}

fn component_prefix(prefix: &str, value: &str) -> bool {
    value == prefix
        || value
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchedulingError {
    EmptyTaskIdentity,
    SelfDependency(String),
    DuplicateTask(String),
    UnknownDependency {
        task_id: String,
        dependency: String,
    },
    DependencyCycle,
    UnknownTask(String),
    UnsafeWritePath(String),
    EmptySemanticKey,
    InvalidSchedulerPolicy,
    InvalidRunWeight,
    ReadySetExceeded {
        ready: usize,
        limit: usize,
    },
    UnknownResourceDemand(String),
    ServiceAccountingOverflow,
    ActiveTaskLimit,
    TaskAlreadyActive(String),
    TaskNotActive(String),
    AdmissionBlocked {
        task_id: String,
        reason: ScheduleBlockReason,
    },
    Transition(TransitionError),
}

impl fmt::Display for SchedulingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTaskIdentity => formatter.write_str("task/run identity must be non-empty"),
            Self::SelfDependency(task) => write!(formatter, "task depends on itself: {task}"),
            Self::DuplicateTask(task) => write!(formatter, "duplicate task: {task}"),
            Self::UnknownDependency {
                task_id,
                dependency,
            } => write!(
                formatter,
                "task {task_id} has unknown dependency {dependency}"
            ),
            Self::DependencyCycle => formatter.write_str("task dependency graph contains a cycle"),
            Self::UnknownTask(task) => write!(formatter, "unknown task: {task}"),
            Self::UnsafeWritePath(path) => {
                write!(formatter, "unsafe repository write path: {path}")
            }
            Self::EmptySemanticKey => formatter.write_str("semantic write key must be non-empty"),
            Self::InvalidSchedulerPolicy => formatter
                .write_str("scheduler policy requires 0 < max_parallel_tasks <= max_ready_tasks"),
            Self::InvalidRunWeight => {
                formatter.write_str("run fairness identity must be non-empty and weight nonzero")
            }
            Self::ReadySetExceeded { ready, limit } => write!(
                formatter,
                "ready task set exceeds configured scheduler bound: {ready} > {limit}"
            ),
            Self::UnknownResourceDemand(task) => {
                write!(formatter, "task has unknown resource demand: {task}")
            }
            Self::ServiceAccountingOverflow => {
                formatter.write_str("weighted fairness service accounting overflow")
            }
            Self::ActiveTaskLimit => formatter.write_str("parallel active-task limit reached"),
            Self::TaskAlreadyActive(task) => write!(formatter, "task already active: {task}"),
            Self::TaskNotActive(task) => write!(formatter, "task is not active: {task}"),
            Self::AdmissionBlocked { task_id, reason } => {
                write!(
                    formatter,
                    "task {task_id} is blocked from admission: {reason:?}"
                )
            }
            Self::Transition(error) => error.fmt(formatter),
        }
    }
}

impl Error for SchedulingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Transition(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TransitionError> for SchedulingError {
    fn from(value: TransitionError) -> Self {
        Self::Transition(value)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use crate::{
        leases::EffectClass,
        resource_governor::{AdmissionClass, ResourceEstimate, ResourceVector},
        state_machines::{TaskState, TaskTransitionContext},
    };

    use super::{
        BoundedScheduler, PreemptionSafety, PrioritySignals, SchedulableTask, SchedulerPolicy,
        SemanticWriteSet, TaskGraph, TaskRisk, WriteScope,
    };

    fn task(id: &str, run: &str, path: &str) -> SchedulableTask {
        SchedulableTask {
            task_id: id.to_owned(),
            run_id: run.to_owned(),
            state: TaskState::Ready,
            dependencies: BTreeSet::new(),
            admission_class: AdmissionClass::Generator,
            resource_estimate: ResourceEstimate::Known(ResourceVector {
                worker_slots: 1,
                memory_bytes: 1,
                provider_concurrency: 1,
                ..ResourceVector::default()
            }),
            effect_class: EffectClass::WorkspaceLocal,
            expected_write_scope: WriteScope::new([path]).expect("scope"),
            semantic_write_set: SemanticWriteSet::default(),
            risk: TaskRisk::Low,
            contracts_stable: true,
            has_local_verifier: true,
            serial_only: false,
            preemption: PreemptionSafety::Checkpointable,
            priority: PrioritySignals::default(),
        }
    }

    #[test]
    fn task_graph_rejects_cycles_and_promotes_dependency_ready_nodes() {
        let mut a = task("a", "run", "src/a.rs");
        a.state = TaskState::Pending;
        a.dependencies.insert("b".to_owned());
        let mut b = task("b", "run", "src/b.rs");
        b.state = TaskState::Pending;
        b.dependencies.insert("a".to_owned());
        assert!(TaskGraph::new(vec![a, b]).is_err());

        let mut root = task("root", "run", "src/root.rs");
        root.state = TaskState::Verifying;
        let mut child = task("child", "run", "src/child.rs");
        child.state = TaskState::Pending;
        child.dependencies.insert("root".to_owned());
        let mut graph = TaskGraph::new(vec![root, child]).expect("graph");
        graph
            .transition_task(
                "root",
                TaskState::Accepted,
                TaskTransitionContext {
                    proof_accepted: true,
                    cancellation_finalized: false,
                },
            )
            .expect("accepted root");
        assert_eq!(graph.refresh_readiness().expect("refresh"), vec!["child"]);
        assert_eq!(graph.task("child").expect("child").state, TaskState::Ready);
    }

    #[test]
    fn write_scope_is_component_aware_and_rejects_host_paths() {
        let auth = WriteScope::new(["src/auth"]).expect("auth scope");
        let token = WriteScope::new(["src/auth/token.rs"]).expect("token scope");
        let authz = WriteScope::new(["src/authz/mod.rs"]).expect("authz scope");
        assert!(auth.overlaps(&token));
        assert!(!auth.overlaps(&authz));
        assert!(WriteScope::new(["../escape"]).is_err());
        assert!(WriteScope::new(["C:\\host\\file"]).is_err());
    }

    #[test]
    fn scheduler_blocks_predicted_and_semantic_conflicts() {
        let first = task("a", "run", "src/auth");
        let mut second = task("b", "run", "src/auth/token.rs");
        second.priority.unblock_value = 10;
        let mut third = task("c", "run", "src/ui");
        third.semantic_write_set = SemanticWriteSet::new(["public-api:auth"]).expect("semantic");
        let mut fourth = task("d", "run", "src/server");
        fourth.semantic_write_set = SemanticWriteSet::new(["public-api:auth"]).expect("semantic");

        let graph = TaskGraph::new(vec![first, second, third, fourth]).expect("graph");
        let policy = SchedulerPolicy::new(4, 8).expect("policy");
        let scheduler = BoundedScheduler::new(policy);
        let wave = scheduler.plan_wave(&graph).expect("wave");
        let selected = wave
            .selected
            .iter()
            .map(|decision| decision.task_id.as_str())
            .collect::<HashSet<_>>();
        assert!(selected.contains("b") || selected.contains("a"));
        assert!(!(selected.contains("a") && selected.contains("b")));
        assert!(!(selected.contains("c") && selected.contains("d")));
    }

    #[test]
    fn weighted_fairness_prevents_one_run_from_taking_every_wave() {
        let a1 = task("a1", "run-a", "a/one");
        let a2 = task("a2", "run-a", "a/two");
        let b1 = task("b1", "run-b", "b/one");
        let b2 = task("b2", "run-b", "b/two");
        let graph =
            TaskGraph::new(vec![a1.clone(), a2.clone(), b1.clone(), b2.clone()]).expect("graph");
        let policy = SchedulerPolicy::new(2, 8).expect("policy");
        let mut scheduler = BoundedScheduler::new(policy);
        let wave = scheduler.plan_wave(&graph).expect("wave");
        assert_eq!(wave.selected.len(), 2);
        let runs = wave
            .selected
            .iter()
            .map(|decision| decision.run_id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(runs.len(), 2);
        for decision in &wave.selected {
            let selected_task = graph.task(&decision.task_id).expect("selected task");
            scheduler
                .record_admission(selected_task)
                .expect("record admission");
        }
        assert_eq!(scheduler.active_count(), 2);
    }

    #[test]
    fn actual_write_expansion_is_reported_before_integration() {
        let a = task("a", "run", "src/a");
        let b = task("b", "run", "src/b");
        let policy = SchedulerPolicy::new(2, 4).expect("policy");
        let mut scheduler = BoundedScheduler::new(policy);
        scheduler.record_admission(&a).expect("a active");
        scheduler.record_admission(&b).expect("b active");
        scheduler
            .observe_actual_write_scope("a", WriteScope::new(["src/shared"]).expect("scope"))
            .expect("first observation");
        let conflicts = scheduler
            .observe_actual_write_scope(
                "b",
                WriteScope::new(["src/shared/file.rs"]).expect("scope"),
            )
            .expect("second observation");
        assert_eq!(conflicts.len(), 1);
        assert!(!conflicts[0].predicted_overlap);
    }

    #[test]
    fn preemption_never_targets_external_mutation_or_non_preemptible_work() {
        let mut safe = task("safe", "run", "src/safe");
        safe.priority.unblock_value = 1;
        let mut external = task("external", "run", "src/external");
        external.effect_class = EffectClass::ExternalMutating;
        external.priority.unblock_value = 0;
        let mut fixed = task("fixed", "run", "src/fixed");
        fixed.preemption = PreemptionSafety::Never;
        fixed.priority.unblock_value = 0;
        let mut incoming = task("incoming", "other", "src/incoming");
        incoming.priority.unblock_value = 100;

        let policy = SchedulerPolicy::new(3, 8).expect("policy");
        let mut scheduler = BoundedScheduler::new(policy);
        scheduler.record_admission(&safe).expect("safe");
        scheduler.record_admission(&external).expect("external");
        scheduler.record_admission(&fixed).expect("fixed");
        assert_eq!(
            scheduler
                .preemption_candidate(&incoming)
                .map(|candidate| candidate.task_id.as_str()),
            Some("safe")
        );
    }
}
