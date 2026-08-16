//! Event-backed long-horizon engineering state, handoff compaction, stagnation detection, and
//! bounded recovery. This module stores conclusions and evidence references, never hidden model
//! reasoning or a duplicate repository index.

use std::{collections::BTreeMap, path::Path};

use aer_repo::RepositoryChangeSet;
use aer_storage::{Causation, DurableState, EventPayload, NewEvent, StorageError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MEMORY_RECORDED: &str = "engineering.memory.recorded";
const MEMORY_INVALIDATED: &str = "engineering.memory.invalidated";
const RECOVERY_ESCALATED: &str = "engineering.recovery.escalated";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryKind {
    VerifiedFact,
    UserDecision,
    SystemDecision,
    Assumption,
    Hypothesis,
    FailureFingerprint,
    ProgressState,
}

impl MemoryKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::VerifiedFact => "verified_fact",
            Self::UserDecision => "user_decision",
            Self::SystemDecision => "system_decision",
            Self::Assumption => "assumption",
            Self::Hypothesis => "hypothesis",
            Self::FailureFingerprint => "failure_fingerprint",
            Self::ProgressState => "progress_state",
        }
    }

    fn parse(value: &str) -> Result<Self, EngineeringStateError> {
        match value {
            "verified_fact" => Ok(Self::VerifiedFact),
            "user_decision" => Ok(Self::UserDecision),
            "system_decision" => Ok(Self::SystemDecision),
            "assumption" => Ok(Self::Assumption),
            "hypothesis" => Ok(Self::Hypothesis),
            "failure_fingerprint" => Ok(Self::FailureFingerprint),
            "progress_state" => Ok(Self::ProgressState),
            _ => Err(EngineeringStateError::InvalidStoredRecord(format!(
                "unknown memory kind {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryValidity {
    Current,
    PotentiallyStale,
    Invalidated,
    Superseded,
}

impl MemoryValidity {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::PotentiallyStale => "potentially_stale",
            Self::Invalidated => "invalidated",
            Self::Superseded => "superseded",
        }
    }

    fn parse(value: &str) -> Result<Self, EngineeringStateError> {
        match value {
            "current" => Ok(Self::Current),
            "potentially_stale" => Ok(Self::PotentiallyStale),
            "invalidated" => Ok(Self::Invalidated),
            "superseded" => Ok(Self::Superseded),
            _ => Err(EngineeringStateError::InvalidStoredRecord(format!(
                "unknown memory validity {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HypothesisState {
    Open,
    Supported,
    Disproven,
    Superseded,
}

impl HypothesisState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Supported => "supported",
            Self::Disproven => "disproven",
            Self::Superseded => "superseded",
        }
    }

    fn parse(value: &str) -> Result<Self, EngineeringStateError> {
        match value {
            "open" => Ok(Self::Open),
            "supported" => Ok(Self::Supported),
            "disproven" => Ok(Self::Disproven),
            "superseded" => Ok(Self::Superseded),
            _ => Err(EngineeringStateError::InvalidStoredRecord(format!(
                "unknown hypothesis state {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureClass {
    SpecificationError,
    ContextMissing,
    ContextMisleading,
    ModelCapabilityLimit,
    ToolFailure,
    EnvironmentFailure,
    ImplementationError,
    VerificationFailure,
    IntegrationFailure,
    SecurityPolicyBlock,
    BudgetExhaustion,
    StagnationLoop,
    ArchitectureDrift,
}

impl FailureClass {
    const fn as_str(self) -> &'static str {
        match self {
            Self::SpecificationError => "specification_error",
            Self::ContextMissing => "context_missing",
            Self::ContextMisleading => "context_misleading",
            Self::ModelCapabilityLimit => "model_capability_limit",
            Self::ToolFailure => "tool_failure",
            Self::EnvironmentFailure => "environment_failure",
            Self::ImplementationError => "implementation_error",
            Self::VerificationFailure => "verification_failure",
            Self::IntegrationFailure => "integration_failure",
            Self::SecurityPolicyBlock => "security_policy_block",
            Self::BudgetExhaustion => "budget_exhaustion",
            Self::StagnationLoop => "stagnation_loop",
            Self::ArchitectureDrift => "architecture_drift",
        }
    }

    fn parse(value: &str) -> Result<Self, EngineeringStateError> {
        match value {
            "specification_error" => Ok(Self::SpecificationError),
            "context_missing" => Ok(Self::ContextMissing),
            "context_misleading" => Ok(Self::ContextMisleading),
            "model_capability_limit" => Ok(Self::ModelCapabilityLimit),
            "tool_failure" => Ok(Self::ToolFailure),
            "environment_failure" => Ok(Self::EnvironmentFailure),
            "implementation_error" => Ok(Self::ImplementationError),
            "verification_failure" => Ok(Self::VerificationFailure),
            "integration_failure" => Ok(Self::IntegrationFailure),
            "security_policy_block" => Ok(Self::SecurityPolicyBlock),
            "budget_exhaustion" => Ok(Self::BudgetExhaustion),
            "stagnation_loop" => Ok(Self::StagnationLoop),
            "architecture_drift" => Ok(Self::ArchitectureDrift),
            _ => Err(EngineeringStateError::InvalidStoredRecord(format!(
                "unknown failure class {value}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryEntityRef {
    pub entity_id: String,
    pub repo_snapshot: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InvalidationScope {
    pub repository_entity_ids: Vec<String>,
    pub spec_versions: Vec<String>,
    pub environment_fingerprints: Vec<String>,
    pub producer_identities: Vec<String>,
    pub dependency_identities: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineeringMemoryRecord {
    pub record_id: String,
    pub kind: MemoryKind,
    pub statement: String,
    pub validity: MemoryValidity,
    pub confidence_milli: u16,
    pub repo_snapshot: Option<String>,
    pub spec_version: Option<String>,
    pub evidence_refs: Vec<String>,
    pub repository_entities: Vec<RepositoryEntityRef>,
    pub invalidation_scope: InvalidationScope,
    pub hypothesis_state: Option<HypothesisState>,
    pub failure_class: Option<FailureClass>,
    pub supersedes: Option<String>,
}

impl EngineeringMemoryRecord {
    pub fn validate(&self) -> Result<(), EngineeringStateError> {
        require_nonempty("record_id", &self.record_id)?;
        require_nonempty("statement", &self.statement)?;
        if self.confidence_milli > 1000 {
            return Err(EngineeringStateError::InvalidInput(
                "confidence_milli must be <= 1000".to_owned(),
            ));
        }
        for reference in &self.evidence_refs {
            require_nonempty("evidence_ref", reference)?;
        }
        for entity in &self.repository_entities {
            require_nonempty("entity_id", &entity.entity_id)?;
            require_nonempty("repo_snapshot", &entity.repo_snapshot)?;
        }
        match self.kind {
            MemoryKind::VerifiedFact if self.evidence_refs.is_empty() => {
                return Err(EngineeringStateError::InvalidInput(
                    "verified facts require evidence_refs".to_owned(),
                ));
            }
            MemoryKind::Hypothesis if self.hypothesis_state.is_none() => {
                return Err(EngineeringStateError::InvalidInput(
                    "hypothesis memory requires hypothesis_state".to_owned(),
                ));
            }
            MemoryKind::FailureFingerprint if self.failure_class.is_none() => {
                return Err(EngineeringStateError::InvalidInput(
                    "failure fingerprint requires failure_class".to_owned(),
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvalidationReason {
    RepositoryChange { from_snapshot: String, to_snapshot: String, entity_ids: Vec<String> },
    SpecChanged { previous: String, current: String },
    EnvironmentChanged { previous: String, current: String },
    ProducerChanged { producer: String },
    DependencyChanged { dependency: String },
    ContradictoryEvidence { evidence_ref: String },
    Superseded { by_record_id: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineeringStateProjection {
    pub project_id: String,
    pub records: Vec<EngineeringMemoryRecord>,
}

impl EngineeringStateProjection {
    pub fn current_records(&self) -> impl Iterator<Item = &EngineeringMemoryRecord> {
        self.records.iter().filter(|record| record.validity == MemoryValidity::Current)
    }

    pub fn backlinks(&self, entity_id: &str) -> Vec<&EngineeringMemoryRecord> {
        self.records
            .iter()
            .filter(|record| record.repository_entities.iter().any(|entity| entity.entity_id == entity_id))
            .collect()
    }
}

pub struct EngineeringStateStore {
    state: DurableState,
    project_id: String,
}

impl EngineeringStateStore {
    pub fn open(state_root: impl AsRef<Path>, project_id: impl Into<String>) -> Result<Self, EngineeringStateError> {
        let project_id = project_id.into();
        require_nonempty("project_id", &project_id)?;
        Ok(Self { state: DurableState::open(state_root)?, project_id })
    }

    pub fn record(&mut self, record: &EngineeringMemoryRecord) -> Result<(), EngineeringStateError> {
        record.validate()?;
        let projection = self.projection()?;
        if projection.records.iter().any(|existing| existing.record_id == record.record_id) {
            return Err(EngineeringStateError::DuplicateRecord(record.record_id.clone()));
        }
        let mut event = NewEvent::new(&self.project_id, MEMORY_RECORDED);
        event.payload = EventPayload::Inline(record_to_json(record));
        self.state.append_event(event)?;
        Ok(())
    }

    pub fn invalidate(&mut self, record_id: &str, reason: InvalidationReason) -> Result<(), EngineeringStateError> {
        require_nonempty("record_id", record_id)?;
        let projection = self.projection()?;
        let record = projection.records.iter().find(|record| record.record_id == record_id)
            .ok_or_else(|| EngineeringStateError::UnknownRecord(record_id.to_owned()))?;
        if matches!(record.validity, MemoryValidity::Invalidated | MemoryValidity::Superseded) {
            return Ok(());
        }
        let validity = if matches!(reason, InvalidationReason::Superseded { .. }) {
            MemoryValidity::Superseded
        } else {
            MemoryValidity::Invalidated
        };
        let mut event = NewEvent::new(&self.project_id, MEMORY_INVALIDATED);
        event.payload = EventPayload::Inline(json!({
            "record_id": record_id,
            "validity": validity.as_str(),
            "reason": invalidation_reason_json(&reason),
        }));
        self.state.append_event(event)?;
        Ok(())
    }

    pub fn invalidate_repository_changes(&mut self, changes: &RepositoryChangeSet) -> Result<Vec<String>, EngineeringStateError> {
        let projection = self.projection()?;
        let changed = changes.invalidated_entity_ids.iter().collect::<std::collections::BTreeSet<_>>();
        let mut invalidated = Vec::new();
        for record in projection.current_records() {
            if record.invalidation_scope.repository_entity_ids.iter().any(|entity| changed.contains(entity)) {
                self.invalidate(
                    &record.record_id,
                    InvalidationReason::RepositoryChange {
                        from_snapshot: changes.from_snapshot.clone(),
                        to_snapshot: changes.to_snapshot.clone(),
                        entity_ids: changes.invalidated_entity_ids.clone(),
                    },
                )?;
                invalidated.push(record.record_id.clone());
            }
        }
        Ok(invalidated)
    }

    pub fn projection(&self) -> Result<EngineeringStateProjection, EngineeringStateError> {
        let mut records: BTreeMap<String, EngineeringMemoryRecord> = BTreeMap::new();
        for event in self.state.events(&self.project_id)? {
            match event.event_type.as_str() {
                MEMORY_RECORDED => {
                    let payload = parse_payload(&event.payload_json)?;
                    let record = record_from_json(payload)?;
                    if records.insert(record.record_id.clone(), record).is_some() {
                        return Err(EngineeringStateError::InvalidStoredRecord(
                            "duplicate record_id in event journal".to_owned(),
                        ));
                    }
                }
                MEMORY_INVALIDATED => {
                    let payload = parse_payload(&event.payload_json)?;
                    let record_id = json_str(payload, "record_id")?;
                    let validity = MemoryValidity::parse(json_str(payload, "validity")?)?;
                    let record = records.get_mut(record_id).ok_or_else(|| {
                        EngineeringStateError::InvalidStoredRecord(format!(
                            "invalidation references unknown record {record_id}"
                        ))
                    })?;
                    record.validity = validity;
                }
                _ => {}
            }
        }
        Ok(EngineeringStateProjection { project_id: self.project_id.clone(), records: records.into_values().collect() })
    }

    pub fn compact_handoff(&self, request: &HandoffRequest, policy: HandoffPolicy) -> Result<HandoffProjection, EngineeringStateError> {
        request.validate()?;
        policy.validate()?;
        let projection = self.projection()?;
        let mut candidates = projection.records.into_iter()
            .filter(|record| record.validity == MemoryValidity::Current)
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| memory_priority(left).cmp(&memory_priority(right)).then_with(|| left.record_id.cmp(&right.record_id)));
        let mut selected = Vec::new();
        let mut used_bytes = base_handoff_bytes(request);
        let mut truncated = false;
        for record in candidates {
            if selected.len() >= policy.max_records {
                truncated = true;
                break;
            }
            let record_bytes = record.statement.len()
                + record.evidence_refs.iter().map(String::len).sum::<usize>()
                + record.repository_entities.iter().map(|entity| entity.entity_id.len() + entity.repo_snapshot.len()).sum::<usize>();
            if used_bytes.saturating_add(record_bytes) > policy.max_bytes {
                truncated = true;
                continue;
            }
            used_bytes = used_bytes.saturating_add(record_bytes);
            selected.push(record);
        }
        let digest = handoff_digest(request, &selected);
        Ok(HandoffProjection {
            handoff_id: format!("handoff:{digest}"),
            task_id: request.task_id.clone(),
            objective: request.objective.clone(),
            spec_version: request.spec_version.clone(),
            repo_snapshot: request.repo_snapshot.clone(),
            requested_action: request.requested_action.clone(),
            records: selected,
            evidence_refs: dedupe_sorted(&request.evidence_refs),
            relevant_context_refs: dedupe_sorted(&request.relevant_context_refs),
            unresolved_dependencies: dedupe_sorted(&request.unresolved_dependencies),
            estimated_tokens: used_bytes.saturating_add(3) / 4,
            truncated,
        })
    }

    pub fn record_recovery_escalation(&mut self, task_id: &str, action: RecoveryAction, failure: FailureClass) -> Result<(), EngineeringStateError> {
        require_nonempty("task_id", task_id)?;
        let mut event = NewEvent::new(&self.project_id, RECOVERY_ESCALATED);
        event.task_id = Some(task_id.to_owned());
        event.payload = EventPayload::Inline(json!({
            "action": action.as_str(),
            "failure_class": failure.as_str(),
        }));
        event.causation = Some(Causation::External(format!("recovery:{task_id}")));
        self.state.append_event(event)?;
        Ok(())
    }

    pub fn verify_integrity(&self) -> Result<(), EngineeringStateError> {
        self.state.verify_project_integrity(&self.project_id)?;
        self.projection()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoffRequest {
    pub task_id: String,
    pub objective: String,
    pub spec_version: String,
    pub repo_snapshot: String,
    pub requested_action: String,
    pub evidence_refs: Vec<String>,
    pub relevant_context_refs: Vec<String>,
    pub unresolved_dependencies: Vec<String>,
}

impl HandoffRequest {
    fn validate(&self) -> Result<(), EngineeringStateError> {
        for (field, value) in [
            ("task_id", self.task_id.as_str()),
            ("objective", self.objective.as_str()),
            ("spec_version", self.spec_version.as_str()),
            ("repo_snapshot", self.repo_snapshot.as_str()),
            ("requested_action", self.requested_action.as_str()),
        ] {
            require_nonempty(field, value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandoffPolicy {
    pub max_records: usize,
    pub max_bytes: usize,
}

impl HandoffPolicy {
    fn validate(self) -> Result<(), EngineeringStateError> {
        if self.max_records == 0 || self.max_bytes == 0 {
            return Err(EngineeringStateError::InvalidInput(
                "handoff bounds must be nonzero".to_owned(),
            ));
        }
        Ok(())
    }
}

impl Default for HandoffPolicy {
    fn default() -> Self {
        Self { max_records: 64, max_bytes: 32 * 1024 }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoffProjection {
    pub handoff_id: String,
    pub task_id: String,
    pub objective: String,
    pub spec_version: String,
    pub repo_snapshot: String,
    pub requested_action: String,
    pub records: Vec<EngineeringMemoryRecord>,
    pub evidence_refs: Vec<String>,
    pub relevant_context_refs: Vec<String>,
    pub unresolved_dependencies: Vec<String>,
    pub estimated_tokens: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgressWindow {
    pub repeated_commands: u32,
    pub repeated_file_reads: u32,
    pub edit_reverts: u32,
    pub new_evidence: u32,
    pub new_relevant_entities: u32,
    pub verifier_improvement_milli: i32,
    pub accepted_subgoals: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagnationPolicy {
    pub repetition_threshold: u32,
    pub max_new_evidence: u32,
    pub max_new_entities: u32,
    pub flat_verifier_delta_milli: i32,
}

impl StagnationPolicy {
    pub fn validate(self) -> Result<Self, EngineeringStateError> {
        if self.repetition_threshold == 0 || self.flat_verifier_delta_milli < 0 {
            return Err(EngineeringStateError::InvalidInput(
                "stagnation policy requires calibrated nonzero repetition and nonnegative verifier delta".to_owned(),
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagnationAssessment {
    pub stagnant: bool,
    pub repetition_score: u32,
    pub evidence_low: bool,
    pub verification_flat: bool,
}

pub fn assess_stagnation(window: ProgressWindow, policy: StagnationPolicy) -> Result<StagnationAssessment, EngineeringStateError> {
    let policy = policy.validate()?;
    let repetition_score = window.repeated_commands
        .saturating_add(window.repeated_file_reads)
        .saturating_add(window.edit_reverts);
    let repetition_high = repetition_score >= policy.repetition_threshold;
    let evidence_low = window.new_evidence <= policy.max_new_evidence
        && window.new_relevant_entities <= policy.max_new_entities
        && window.accepted_subgoals == 0;
    let verification_flat = window.verifier_improvement_milli.abs() <= policy.flat_verifier_delta_milli;
    Ok(StagnationAssessment {
        stagnant: repetition_high && evidence_low && verification_flat,
        repetition_score,
        evidence_low,
        verification_flat,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    ToolRetry,
    ContextRefresh,
    HypothesisReset,
    FreshContextTakeover,
    ModelEscalation,
    TopologyChange,
    RollbackAlternative,
    UserIntervention,
}

impl RecoveryAction {
    const LADDER: [Self; 8] = [
        Self::ToolRetry,
        Self::ContextRefresh,
        Self::HypothesisReset,
        Self::FreshContextTakeover,
        Self::ModelEscalation,
        Self::TopologyChange,
        Self::RollbackAlternative,
        Self::UserIntervention,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::ToolRetry => "tool_retry",
            Self::ContextRefresh => "context_refresh",
            Self::HypothesisReset => "hypothesis_reset",
            Self::FreshContextTakeover => "fresh_context_takeover",
            Self::ModelEscalation => "model_escalation",
            Self::TopologyChange => "topology_change",
            Self::RollbackAlternative => "rollback_alternative",
            Self::UserIntervention => "user_intervention",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryBudget {
    pub maximum_escalations: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryState {
    pub escalations_used: u8,
}

impl RecoveryState {
    pub fn next(self, budget: RecoveryBudget) -> Option<RecoveryAction> {
        if self.escalations_used >= budget.maximum_escalations {
            return None;
        }
        RecoveryAction::LADDER.get(usize::from(self.escalations_used)).copied()
    }

    pub fn advance(self, budget: RecoveryBudget) -> Option<Self> {
        self.next(budget)?;
        Some(Self { escalations_used: self.escalations_used.saturating_add(1) })
    }
}

#[derive(Debug)]
pub enum EngineeringStateError {
    Storage(StorageError),
    InvalidInput(String),
    InvalidStoredRecord(String),
    DuplicateRecord(String),
    UnknownRecord(String),
}

impl std::fmt::Display for EngineeringStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(error) => write!(formatter, "engineering state storage failed: {error}"),
            Self::InvalidInput(error) => write!(formatter, "invalid engineering state input: {error}"),
            Self::InvalidStoredRecord(error) => write!(formatter, "invalid stored engineering state: {error}"),
            Self::DuplicateRecord(id) => write!(formatter, "engineering memory record already exists: {id}"),
            Self::UnknownRecord(id) => write!(formatter, "engineering memory record does not exist: {id}"),
        }
    }
}

impl std::error::Error for EngineeringStateError {}

impl From<StorageError> for EngineeringStateError {
    fn from(value: StorageError) -> Self { Self::Storage(value) }
}

fn memory_priority(record: &EngineeringMemoryRecord) -> u8 {
    match (record.kind, record.hypothesis_state) {
        (MemoryKind::UserDecision, _) => 0,
        (MemoryKind::VerifiedFact, _) => 1,
        (MemoryKind::FailureFingerprint, _) => 2,
        (MemoryKind::Hypothesis, Some(HypothesisState::Disproven)) => 3,
        (MemoryKind::Hypothesis, _) => 4,
        (MemoryKind::SystemDecision, _) => 5,
        (MemoryKind::ProgressState, _) => 6,
        (MemoryKind::Assumption, _) => 7,
    }
}

fn base_handoff_bytes(request: &HandoffRequest) -> usize {
    request.task_id.len()
        + request.objective.len()
        + request.spec_version.len()
        + request.repo_snapshot.len()
        + request.requested_action.len()
}

fn handoff_digest(request: &HandoffRequest, records: &[EngineeringMemoryRecord]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"aer-handoff-compaction-v1\0");
    for value in [
        request.task_id.as_str(), request.objective.as_str(), request.spec_version.as_str(),
        request.repo_snapshot.as_str(), request.requested_action.as_str(),
    ] {
        digest.update(value.as_bytes());
        digest.update(b"\0");
    }
    for record in records {
        digest.update(record.record_id.as_bytes());
        digest.update(b"\0");
        digest.update(record.statement.as_bytes());
        digest.update(b"\0");
    }
    hex(digest.finalize().as_ref())
}

fn dedupe_sorted(values: &[String]) -> Vec<String> {
    let mut values = values.to_vec();
    values.sort();
    values.dedup();
    values
}

fn record_to_json(record: &EngineeringMemoryRecord) -> Value {
    json!({
        "record_id": record.record_id,
        "kind": record.kind.as_str(),
        "statement": record.statement,
        "validity": record.validity.as_str(),
        "confidence_milli": record.confidence_milli,
        "repo_snapshot": record.repo_snapshot,
        "spec_version": record.spec_version,
        "evidence_refs": record.evidence_refs,
        "repository_entities": record.repository_entities.iter().map(|entity| json!({
            "entity_id": entity.entity_id,
            "repo_snapshot": entity.repo_snapshot,
        })).collect::<Vec<_>>(),
        "invalidation_scope": {
            "repository_entity_ids": record.invalidation_scope.repository_entity_ids,
            "spec_versions": record.invalidation_scope.spec_versions,
            "environment_fingerprints": record.invalidation_scope.environment_fingerprints,
            "producer_identities": record.invalidation_scope.producer_identities,
            "dependency_identities": record.invalidation_scope.dependency_identities,
        },
        "hypothesis_state": record.hypothesis_state.map(HypothesisState::as_str),
        "failure_class": record.failure_class.map(FailureClass::as_str),
        "supersedes": record.supersedes,
    })
}

fn record_from_json(value: &Value) -> Result<EngineeringMemoryRecord, EngineeringStateError> {
    let entities = json_array(value, "repository_entities")?.iter().map(|entity| {
        Ok(RepositoryEntityRef {
            entity_id: json_str(entity, "entity_id")?.to_owned(),
            repo_snapshot: json_str(entity, "repo_snapshot")?.to_owned(),
        })
    }).collect::<Result<Vec<_>, EngineeringStateError>>()?;
    let scope = value.get("invalidation_scope").ok_or_else(|| EngineeringStateError::InvalidStoredRecord("missing invalidation_scope".to_owned()))?;
    let record = EngineeringMemoryRecord {
        record_id: json_str(value, "record_id")?.to_owned(),
        kind: MemoryKind::parse(json_str(value, "kind")?)?,
        statement: json_str(value, "statement")?.to_owned(),
        validity: MemoryValidity::parse(json_str(value, "validity")?)?,
        confidence_milli: u16::try_from(json_u64(value, "confidence_milli")?).map_err(|_| EngineeringStateError::InvalidStoredRecord("confidence overflow".to_owned()))?,
        repo_snapshot: json_optional_str(value, "repo_snapshot")?.map(str::to_owned),
        spec_version: json_optional_str(value, "spec_version")?.map(str::to_owned),
        evidence_refs: json_string_array(value, "evidence_refs")?,
        repository_entities: entities,
        invalidation_scope: InvalidationScope {
            repository_entity_ids: json_string_array(scope, "repository_entity_ids")?,
            spec_versions: json_string_array(scope, "spec_versions")?,
            environment_fingerprints: json_string_array(scope, "environment_fingerprints")?,
            producer_identities: json_string_array(scope, "producer_identities")?,
            dependency_identities: json_string_array(scope, "dependency_identities")?,
        },
        hypothesis_state: json_optional_str(value, "hypothesis_state")?.map(HypothesisState::parse).transpose()?,
        failure_class: json_optional_str(value, "failure_class")?.map(FailureClass::parse).transpose()?,
        supersedes: json_optional_str(value, "supersedes")?.map(str::to_owned),
    };
    record.validate()?;
    Ok(record)
}

fn invalidation_reason_json(reason: &InvalidationReason) -> Value {
    match reason {
        InvalidationReason::RepositoryChange { from_snapshot, to_snapshot, entity_ids } => json!({"kind":"repository_change","from_snapshot":from_snapshot,"to_snapshot":to_snapshot,"entity_ids":entity_ids}),
        InvalidationReason::SpecChanged { previous, current } => json!({"kind":"spec_changed","previous":previous,"current":current}),
        InvalidationReason::EnvironmentChanged { previous, current } => json!({"kind":"environment_changed","previous":previous,"current":current}),
        InvalidationReason::ProducerChanged { producer } => json!({"kind":"producer_changed","producer":producer}),
        InvalidationReason::DependencyChanged { dependency } => json!({"kind":"dependency_changed","dependency":dependency}),
        InvalidationReason::ContradictoryEvidence { evidence_ref } => json!({"kind":"contradictory_evidence","evidence_ref":evidence_ref}),
        InvalidationReason::Superseded { by_record_id } => json!({"kind":"superseded","by_record_id":by_record_id}),
    }
}

fn parse_payload(value: &Option<String>) -> Result<&Value, EngineeringStateError> {
    let _ = value;
    Err(EngineeringStateError::InvalidStoredRecord(
        "internal payload parser placeholder must be replaced before validation".to_owned(),
    ))
}

fn json_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, EngineeringStateError> {
    value.get(key).and_then(Value::as_str).ok_or_else(|| EngineeringStateError::InvalidStoredRecord(format!("missing string field {key}")))
}

fn json_optional_str<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>, EngineeringStateError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_str().map(Some).ok_or_else(|| EngineeringStateError::InvalidStoredRecord(format!("field {key} must be string or null"))),
    }
}

fn json_u64(value: &Value, key: &str) -> Result<u64, EngineeringStateError> {
    value.get(key).and_then(Value::as_u64).ok_or_else(|| EngineeringStateError::InvalidStoredRecord(format!("missing integer field {key}")))
}

fn json_array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, EngineeringStateError> {
    value.get(key).and_then(Value::as_array).ok_or_else(|| EngineeringStateError::InvalidStoredRecord(format!("missing array field {key}")))
}

fn json_string_array(value: &Value, key: &str) -> Result<Vec<String>, EngineeringStateError> {
    json_array(value, key)?.iter().map(|item| item.as_str().map(str::to_owned).ok_or_else(|| EngineeringStateError::InvalidStoredRecord(format!("array {key} contains non-string")))).collect()
}

fn require_nonempty(field: &str, value: &str) -> Result<(), EngineeringStateError> {
    if value.trim().is_empty() { Err(EngineeringStateError::InvalidInput(format!("{field} cannot be empty"))) } else { Ok(()) }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stagnation_requires_all_three_documented_signals() {
        let policy = StagnationPolicy { repetition_threshold: 5, max_new_evidence: 1, max_new_entities: 1, flat_verifier_delta_milli: 10 };
        let stagnant = assess_stagnation(ProgressWindow {
            repeated_commands: 3, repeated_file_reads: 2, edit_reverts: 1,
            new_evidence: 0, new_relevant_entities: 0, verifier_improvement_milli: 0, accepted_subgoals: 0,
        }, policy).expect("assessment");
        assert!(stagnant.stagnant);
        let progressing = assess_stagnation(ProgressWindow { new_evidence: 2, ..ProgressWindow {
            repeated_commands: 3, repeated_file_reads: 2, edit_reverts: 1,
            new_evidence: 0, new_relevant_entities: 0, verifier_improvement_milli: 0, accepted_subgoals: 0,
        } }, policy).expect("assessment");
        assert!(!progressing.stagnant);
    }

    #[test]
    fn recovery_ladder_is_bounded_and_ordered() {
        let budget = RecoveryBudget { maximum_escalations: 3 };
        let mut state = RecoveryState { escalations_used: 0 };
        assert_eq!(state.next(budget), Some(RecoveryAction::ToolRetry));
        state = state.advance(budget).expect("advance");
        assert_eq!(state.next(budget), Some(RecoveryAction::ContextRefresh));
        state = state.advance(budget).expect("advance");
        assert_eq!(state.next(budget), Some(RecoveryAction::HypothesisReset));
        state = state.advance(budget).expect("advance");
        assert_eq!(state.next(budget), None);
    }
}
