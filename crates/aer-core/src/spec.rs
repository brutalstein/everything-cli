//! Phase-2 intent, research-evidence, Engineering-IR, and SpecDelta application boundary.

use std::{collections::BTreeMap, error::Error, fmt, path::Path};

use aer_contracts::{
    embedded::EmbeddedContractRegistry, semantic::SemanticBundle, validate_semantic_bundle,
};
use aer_domain::{
    contracts::CoreContract,
    spec::{
        AcceptanceCriterion, ChecksumSeverity, Decision, DecisionAuthority, EngineeringIr,
        IntentState, ProjectDescriptor, Reversibility, Risk, SemanticChecksum, SemanticItem,
        SemanticStatus, SourceKind, SourceRef, SpecDelta, Unknown, UnknownResolution, UserMessage,
        semantic_checksum,
    },
};
use aer_research::ValidatedResearchArtifact;
use aer_storage::{DurableState, EventPayload, NewEvent, ObjectMetadata, Sensitivity, StoredEvent};
use aer_workspace::WorkspaceIdentity;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{open_store, project_runtime_root};

const MAX_USER_INPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSemanticKind {
    Goal,
    NonGoal,
    Constraint,
    Assumption,
    QualityAttribute,
    AcceptanceCriterion,
}

impl UserSemanticKind {
    const fn event_name(self) -> &'static str {
        match self {
            Self::Goal => "goal",
            Self::NonGoal => "non_goal",
            Self::Constraint => "constraint",
            Self::Assumption => "assumption",
            Self::QualityAttribute => "quality_attribute",
            Self::AcceptanceCriterion => "acceptance_criterion",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecSnapshot {
    pub project_id: String,
    pub revision: u64,
    pub intent: IntentState,
    pub ir: Option<EngineeringIr>,
    pub checksum: Option<SemanticChecksum>,
    pub research_artifact_count: usize,
    pub latest_delta: Option<SpecDelta>,
}

impl SpecSnapshot {
    #[must_use]
    pub fn open_unknown_count(&self) -> usize {
        self.intent.unknowns.len()
    }

    #[must_use]
    pub fn next_question(&self) -> Option<&Unknown> {
        self.intent.next_user_question()
    }
}

pub struct SpecService;

impl SpecService {
    pub fn inspect(
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
    ) -> Result<SpecSnapshot, SpecError> {
        let workspace = inspect_workspace(workspace_root.as_ref())?;
        let project_root = project_runtime_root(state_home.as_ref(), &workspace.repo_id);
        if !project_root.join("durable").join(".aer").exists() {
            return Ok(empty_snapshot(&workspace.repo_id));
        }
        let store = open_store(&project_root)?;
        compile_snapshot(&store, &workspace)
    }

    /// Records natural-language user intent without pretending an unavailable
    /// language model performed semantic extraction. The first greenfield message
    /// becomes the user-origin goal; later messages remain provenance until an
    /// explicit semantic action or future extraction adapter classifies them.
    pub fn submit_message(
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        text: &str,
    ) -> Result<SpecSnapshot, SpecError> {
        validate_user_text(text)?;
        let workspace = inspect_workspace(workspace_root.as_ref())?;
        let project_root = project_runtime_root(state_home.as_ref(), &workspace.repo_id);
        let mut store = open_store(&project_root)?;
        let before = compile_snapshot(&store, &workspace)?;
        let source_event = append_user_message(&mut store, &workspace.repo_id, text)?;
        persist_compilation(&mut store, &workspace, &before, source_ref(&source_event))
    }

    /// Records an explicitly classified semantic statement under user authority.
    pub fn record_semantic(
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        kind: UserSemanticKind,
        statement: &str,
    ) -> Result<SpecSnapshot, SpecError> {
        validate_user_text(statement)?;
        let workspace = inspect_workspace(workspace_root.as_ref())?;
        let project_root = project_runtime_root(state_home.as_ref(), &workspace.repo_id);
        let mut store = open_store(&project_root)?;
        let before = compile_snapshot(&store, &workspace)?;
        let source_event = append_user_message(&mut store, &workspace.repo_id, statement)?;
        let id = stable_id(kind_prefix(kind), &workspace.repo_id, statement);
        let mut event = NewEvent::new(&workspace.repo_id, "intent.semantic.recorded");
        event.payload = EventPayload::Inline(json!({
            "kind": kind.event_name(),
            "id": id,
            "statement": statement,
            "source_event_id": source_event.event_id,
        }));
        store.append_event(event)?;
        persist_compilation(&mut store, &workspace, &before, source_ref(&source_event))
    }

    pub fn record_user_decision(
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        choice: &str,
    ) -> Result<SpecSnapshot, SpecError> {
        validate_user_text(choice)?;
        let workspace = inspect_workspace(workspace_root.as_ref())?;
        let project_root = project_runtime_root(state_home.as_ref(), &workspace.repo_id);
        let mut store = open_store(&project_root)?;
        let before = compile_snapshot(&store, &workspace)?;
        let source_event = append_user_message(&mut store, &workspace.repo_id, choice)?;
        let id = stable_id("DEC", &workspace.repo_id, choice);
        let mut event = NewEvent::new(&workspace.repo_id, "intent.decision.recorded");
        event.payload = EventPayload::Inline(json!({
            "id": id,
            "choice": choice,
            "source_event_id": source_event.event_id,
        }));
        store.append_event(event)?;
        persist_compilation(&mut store, &workspace, &before, source_ref(&source_event))
    }

    /// Ingests an already acquired ResearchArtifact. Network/search acquisition is
    /// intentionally a separate adapter boundary; this method never fabricates
    /// search results and never promotes external claims into accepted semantics.
    pub fn ingest_research(
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        artifact: Value,
    ) -> Result<SpecSnapshot, SpecError> {
        let artifact = ValidatedResearchArtifact::ingest_untrusted(artifact)?;
        let workspace = inspect_workspace(workspace_root.as_ref())?;
        let project_root = project_runtime_root(state_home.as_ref(), &workspace.repo_id);
        let mut store = open_store(&project_root)?;
        let before = compile_snapshot(&store, &workspace)?;
        let bytes = artifact.canonical_bytes()?;
        let hash = store.put_object(
            &workspace.repo_id,
            &bytes,
            &ObjectMetadata {
                sensitivity: Sensitivity::Internal,
                retention_class: "research-artifact".to_owned(),
                expires_at: None,
                pinned: true,
            },
        )?;
        let mut event = NewEvent::new(&workspace.repo_id, "research.artifact.recorded");
        event.payload = EventPayload::Artifact(hash);
        let stored = store.append_event(event)?;
        let source = SourceRef {
            kind: SourceKind::ResearchClaim,
            id: artifact.research_id().to_owned(),
            detail: Some(stored.event_id),
        };
        persist_compilation(&mut store, &workspace, &before, source)
    }
}

fn persist_compilation(
    store: &mut DurableState,
    workspace: &WorkspaceIdentity,
    before: &SpecSnapshot,
    source: SourceRef,
) -> Result<SpecSnapshot, SpecError> {
    let mut after = compile_snapshot(store, workspace)?;
    let Some(ir) = after.ir.as_ref() else {
        return Ok(after);
    };
    let checksum = after
        .checksum
        .as_ref()
        .expect("compiled IR always has semantic checksum");
    if checksum.severity == ChecksumSeverity::High {
        return Err(SpecError::SemanticChecksum(checksum.clone()));
    }

    let revision = before.revision + 1;
    let document = engineering_ir_json(ir, revision);
    validate_engineering_ir(&document)?;
    let bytes = serde_json::to_vec(&document)?;
    let hash = store.put_object(
        &workspace.repo_id,
        &bytes,
        &ObjectMetadata {
            sensitivity: Sensitivity::Internal,
            retention_class: "engineering-ir".to_owned(),
            expires_at: None,
            pinned: true,
        },
    )?;
    let mut ir_event = NewEvent::new(&workspace.repo_id, "spec.ir.document");
    ir_event.payload = EventPayload::Artifact(hash);
    store.append_event(ir_event)?;

    let delta = build_delta(before.ir.as_ref(), ir, before.revision, revision, source);
    let mut delta_event = NewEvent::new(&workspace.repo_id, "spec.delta.recorded");
    delta_event.payload = EventPayload::Inline(spec_delta_json(&delta));
    store.append_event(delta_event)?;
    store.verify_project_integrity(&workspace.repo_id)?;

    after.revision = revision;
    after.latest_delta = Some(delta);
    Ok(after)
}

fn compile_snapshot(
    store: &DurableState,
    workspace: &WorkspaceIdentity,
) -> Result<SpecSnapshot, SpecError> {
    let events = store.events(&workspace.repo_id)?;
    let revision = events
        .iter()
        .filter(|event| event.event_type == "spec.ir.document")
        .count() as u64;
    let mut intent = IntentState::empty();
    let mut research_findings = Vec::new();
    let mut research_artifact_count = 0_usize;

    for event in &events {
        match event.event_type.as_str() {
            "intent.user_message" => {
                let hash = event.payload_artifact_hash.as_ref().ok_or_else(|| {
                    SpecError::Integrity("intent.user_message requires artifact payload".to_owned())
                })?;
                let bytes = store.read_object(&workspace.repo_id, hash)?;
                let text = String::from_utf8(bytes).map_err(|_| {
                    SpecError::Integrity("intent message artifact is not UTF-8".to_owned())
                })?;
                intent.messages.push(UserMessage {
                    id: event.event_id.clone(),
                    text,
                });
            }
            "intent.semantic.recorded" => apply_semantic_event(event, &mut intent)?,
            "intent.decision.recorded" => apply_decision_event(event, &mut intent)?,
            "research.artifact.recorded" => {
                let hash = event.payload_artifact_hash.as_ref().ok_or_else(|| {
                    SpecError::Integrity(
                        "research.artifact.recorded requires artifact payload".to_owned(),
                    )
                })?;
                let bytes = store.read_object(&workspace.repo_id, hash)?;
                let value: Value = serde_json::from_slice(&bytes)?;
                let artifact = ValidatedResearchArtifact::ingest_untrusted(value)?;
                research_findings.extend_from_slice(artifact.findings());
                research_artifact_count += 1;
            }
            _ => {}
        }
    }

    derive_minimum_intent(&workspace.repo_id, &mut intent);
    if intent.messages.is_empty()
        && intent.goals.is_empty()
        && intent.constraints.is_empty()
        && intent.acceptance_criteria.is_empty()
        && intent.user_decisions.is_empty()
    {
        return Ok(SpecSnapshot {
            project_id: workspace.repo_id.clone(),
            revision,
            intent,
            ir: None,
            checksum: None,
            research_artifact_count,
            latest_delta: latest_delta(&events)?,
        });
    }

    let summary = intent
        .messages
        .first()
        .map(|message| message.text.clone())
        .or_else(|| intent.goals.first().map(|goal| goal.statement.clone()))
        .ok_or_else(|| SpecError::Integrity("intent has no project summary source".to_owned()))?;
    let title = workspace
        .repo_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_owned());
    let mut decisions = intent.user_decisions.clone();
    decisions.extend(intent.system_decisions.clone());
    let ir = EngineeringIr {
        schema_version: 1,
        project: ProjectDescriptor {
            id: workspace.repo_id.clone(),
            title,
            summary,
        },
        goals: intent.goals.clone(),
        non_goals: intent.non_goals.clone(),
        functional_requirements: intent.functional_requirements.clone(),
        quality_attributes: intent.quality_attributes.clone(),
        constraints: intent.constraints.clone(),
        invariants: Vec::new(),
        acceptance_criteria: intent.acceptance_criteria.clone(),
        risks: intent.risks.clone(),
        decisions,
        unknowns: intent.unknowns.clone(),
        assumptions: intent.assumptions.clone(),
        research_findings,
    };
    let document = engineering_ir_json(&ir, revision.max(1));
    validate_engineering_ir(&document)?;
    let checksum = semantic_checksum(&intent, &ir);

    Ok(SpecSnapshot {
        project_id: workspace.repo_id.clone(),
        revision,
        intent,
        ir: Some(ir),
        checksum: Some(checksum),
        research_artifact_count,
        latest_delta: latest_delta(&events)?,
    })
}

fn validate_engineering_ir(document: &Value) -> Result<(), SpecError> {
    let registry =
        EmbeddedContractRegistry::load().map_err(|error| SpecError::Contract(error.to_string()))?;
    registry
        .validate_current(CoreContract::EngineeringIr, document)
        .map_err(|error| SpecError::Contract(error.to_string()))?;
    let issues = validate_semantic_bundle(&SemanticBundle {
        engineering_ir: document.clone(),
        tasks: Vec::new(),
        evidence: Vec::new(),
        proof_manifests: Vec::new(),
    });
    if issues.is_empty() {
        Ok(())
    } else {
        Err(SpecError::Semantic(
            issues
                .into_iter()
                .map(|issue| format!("{} {}: {}", issue.code, issue.path, issue.message))
                .collect(),
        ))
    }
}

fn derive_minimum_intent(project_id: &str, intent: &mut IntentState) {
    if intent.goals.is_empty()
        && let Some(message) = intent.messages.first()
    {
        intent.goals.push(SemanticItem {
            id: stable_id("GOAL", project_id, &message.text),
            statement: message.text.clone(),
            source_refs: vec![SourceRef {
                kind: SourceKind::UserMessage,
                id: message.id.clone(),
                detail: Some("greenfield intent preserved verbatim".to_owned()),
            }],
            status: SemanticStatus::Accepted,
            risk: Risk::Medium,
        });
    }

    if intent.acceptance_criteria.is_empty()
        && !intent.goals.is_empty()
        && !intent
            .unknowns
            .iter()
            .any(|unknown| unknown.id.starts_with("UNK-ACCEPTANCE-"))
    {
        intent.unknowns.push(Unknown {
            id: stable_id(
                "UNK-ACCEPTANCE",
                project_id,
                "observable completion criteria",
            ),
            question: "What observable outcome should prove this request is complete?".to_owned(),
            uncertainty_milli: 1000,
            impact_milli: 1000,
            irreversibility_milli: 700,
            friction_milli: 200,
            resolution: UnknownResolution::AskUser,
            evidence_refs: intent
                .messages
                .first()
                .map(|message| SourceRef {
                    kind: SourceKind::UserMessage,
                    id: message.id.clone(),
                    detail: None,
                })
                .into_iter()
                .collect(),
        });
    }
}

fn append_user_message(
    store: &mut DurableState,
    project_id: &str,
    text: &str,
) -> Result<StoredEvent, SpecError> {
    let hash = store.put_object(
        project_id,
        text.as_bytes(),
        &ObjectMetadata {
            sensitivity: Sensitivity::Internal,
            retention_class: "intent-user-message".to_owned(),
            expires_at: None,
            pinned: true,
        },
    )?;
    let mut event = NewEvent::new(project_id, "intent.user_message");
    event.payload = EventPayload::Artifact(hash);
    store.append_event(event).map_err(SpecError::from)
}

fn apply_semantic_event(event: &StoredEvent, intent: &mut IntentState) -> Result<(), SpecError> {
    let value = inline_json(event)?;
    let kind = required_string(&value, "kind")?;
    let source_event_id = required_string(&value, "source_event_id")?;
    let item = SemanticItem {
        id: required_string(&value, "id")?.to_owned(),
        statement: required_string(&value, "statement")?.to_owned(),
        source_refs: vec![SourceRef {
            kind: SourceKind::UserMessage,
            id: source_event_id.to_owned(),
            detail: Some(format!("explicit user classification: {kind}")),
        }],
        status: SemanticStatus::Accepted,
        risk: if kind == "acceptance_criterion" {
            Risk::High
        } else {
            Risk::Medium
        },
    };
    match kind {
        "goal" => push_unique(&mut intent.goals, item),
        "non_goal" => push_unique(&mut intent.non_goals, item),
        "constraint" => push_unique(&mut intent.constraints, item),
        "assumption" => push_unique(&mut intent.assumptions, item),
        "quality_attribute" => push_unique(&mut intent.quality_attributes, item),
        "acceptance_criterion" => {
            if !intent
                .acceptance_criteria
                .iter()
                .any(|criterion| criterion.item.id == item.id)
            {
                intent.acceptance_criteria.push(AcceptanceCriterion {
                    item,
                    requirement_refs: Vec::new(),
                });
            }
        }
        unknown => {
            return Err(SpecError::Integrity(format!(
                "unknown intent semantic kind: {unknown}"
            )));
        }
    }
    Ok(())
}

fn apply_decision_event(event: &StoredEvent, intent: &mut IntentState) -> Result<(), SpecError> {
    let value = inline_json(event)?;
    let source_event_id = required_string(&value, "source_event_id")?;
    let decision = Decision {
        id: required_string(&value, "id")?.to_owned(),
        choice: required_string(&value, "choice")?.to_owned(),
        authority: DecisionAuthority::User,
        rationale: "Explicitly supplied by the user.".to_owned(),
        confidence_milli: 1000,
        reversibility: Reversibility::Moderate,
        source_refs: vec![SourceRef {
            kind: SourceKind::UserMessage,
            id: source_event_id.to_owned(),
            detail: Some("explicit user decision".to_owned()),
        }],
    };
    if !intent
        .user_decisions
        .iter()
        .any(|existing| existing.id == decision.id)
    {
        intent.user_decisions.push(decision);
    }
    Ok(())
}

fn inline_json(event: &StoredEvent) -> Result<Value, SpecError> {
    let text = event.payload_json.as_deref().ok_or_else(|| {
        SpecError::Integrity(format!("{} requires inline payload", event.event_type))
    })?;
    serde_json::from_str(text).map_err(SpecError::from)
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, SpecError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| SpecError::Integrity(format!("event field {field} is missing")))
}

fn push_unique(items: &mut Vec<SemanticItem>, item: SemanticItem) {
    if !items.iter().any(|existing| existing.id == item.id) {
        items.push(item);
    }
}

fn inspect_workspace(path: &Path) -> Result<WorkspaceIdentity, SpecError> {
    WorkspaceIdentity::inspect(path).map_err(|error| SpecError::LowerLayer {
        context: "workspace identity",
        message: error.to_string(),
    })
}

fn validate_user_text(text: &str) -> Result<(), SpecError> {
    if text.trim().is_empty() {
        return Err(SpecError::InvalidInput("input must not be empty"));
    }
    if text.len() > MAX_USER_INPUT_BYTES {
        return Err(SpecError::InvalidInput("input exceeds 64 KiB"));
    }
    Ok(())
}

fn source_ref(event: &StoredEvent) -> SourceRef {
    SourceRef {
        kind: SourceKind::UserMessage,
        id: event.event_id.clone(),
        detail: None,
    }
}

fn kind_prefix(kind: UserSemanticKind) -> &'static str {
    match kind {
        UserSemanticKind::Goal => "GOAL",
        UserSemanticKind::NonGoal => "NONGOAL",
        UserSemanticKind::Constraint => "CONSTRAINT",
        UserSemanticKind::Assumption => "ASSUMPTION",
        UserSemanticKind::QualityAttribute => "QA",
        UserSemanticKind::AcceptanceCriterion => "AC",
    }
}

fn stable_id(prefix: &str, project_id: &str, statement: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"EVERYTHING_SPEC_ID_V1\0");
    hasher.update(project_id.as_bytes());
    hasher.update([0]);
    hasher.update(prefix.as_bytes());
    hasher.update([0]);
    hasher.update(statement.trim().as_bytes());
    let digest = hasher.finalize();
    let suffix = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    format!("{prefix}-{suffix}")
}

fn engineering_ir_json(ir: &EngineeringIr, revision: u64) -> Value {
    json!({
        "schema_version": ir.schema_version,
        "project": {
            "id": ir.project.id,
            "title": ir.project.title,
            "summary": ir.project.summary,
            "revision": revision,
        },
        "goals": ir.goals.iter().map(semantic_item_json).collect::<Vec<_>>(),
        "non_goals": ir.non_goals.iter().map(semantic_item_json).collect::<Vec<_>>(),
        "stakeholders": [],
        "user_flows": [],
        "functional_requirements": ir.functional_requirements.iter().map(|requirement| {
            let mut value = semantic_item_json(&requirement.item);
            if let Some(object) = value.as_object_mut() {
                object.insert("priority".to_owned(), json!(match requirement.priority {
                    aer_domain::spec::RequirementPriority::Must => "must",
                    aer_domain::spec::RequirementPriority::Should => "should",
                    aer_domain::spec::RequirementPriority::Could => "could",
                }));
                object.insert("verification_strategy".to_owned(), json!(requirement.verification_strategy));
                object.insert("dependencies".to_owned(), json!(requirement.dependencies));
            }
            value
        }).collect::<Vec<_>>(),
        "quality_attributes": ir.quality_attributes.iter().map(semantic_item_json).collect::<Vec<_>>(),
        "constraints": ir.constraints.iter().map(semantic_item_json).collect::<Vec<_>>(),
        "invariants": ir.invariants.iter().map(semantic_item_json).collect::<Vec<_>>(),
        "acceptance_criteria": ir.acceptance_criteria.iter().map(|criterion| {
            let mut value = semantic_item_json(&criterion.item);
            if let Some(object) = value.as_object_mut() {
                object.insert("requirement_refs".to_owned(), json!(criterion.requirement_refs));
            }
            value
        }).collect::<Vec<_>>(),
        "interfaces": [],
        "data_contracts": [],
        "security_privacy": [],
        "performance_targets": [],
        "compatibility": [],
        "risks": ir.risks.iter().map(semantic_item_json).collect::<Vec<_>>(),
        "decisions": ir.decisions.iter().map(decision_json).collect::<Vec<_>>(),
        "unknowns": ir.unknowns.iter().map(unknown_json).collect::<Vec<_>>(),
        "assumptions": ir.assumptions.iter().map(semantic_item_json).collect::<Vec<_>>(),
        "research_findings": ir.research_findings.iter().map(|finding| json!({
            "research_id": finding.research_id,
            "claim_id": finding.claim_id,
            "statement": finding.statement,
            "status": match finding.status {
                aer_domain::spec::ResearchClaimStatus::Supported => "supported",
                aer_domain::spec::ResearchClaimStatus::Contested => "contested",
                aer_domain::spec::ResearchClaimStatus::Insufficient => "insufficient",
                aer_domain::spec::ResearchClaimStatus::Superseded => "superseded",
            },
            "confidence": f64::from(finding.confidence_milli) / 1000.0,
            "source_refs": finding.source_refs,
            "authority": "external_evidence",
            "promotion": "proposed_only",
        })).collect::<Vec<_>>(),
    })
}

fn semantic_item_json(item: &SemanticItem) -> Value {
    json!({
        "id": item.id,
        "statement": item.statement,
        "source_refs": item.source_refs.iter().map(source_ref_json).collect::<Vec<_>>(),
        "status": match item.status {
            SemanticStatus::Proposed => "proposed",
            SemanticStatus::Accepted => "accepted",
            SemanticStatus::Deprecated => "deprecated",
        },
        "risk": match item.risk {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
            Risk::Critical => "critical",
        },
    })
}

fn source_ref_json(source: &SourceRef) -> Value {
    let mut value = json!({
        "type": match source.kind {
            SourceKind::UserMessage => "user_message",
            SourceKind::ResearchClaim => "research_claim",
            SourceKind::SystemDefault => "system_default",
            SourceKind::Repository => "repository",
            SourceKind::ArchitectureDecision => "adr",
        },
        "id": source.id,
    });
    if let Some(detail) = source.detail.as_deref() {
        value
            .as_object_mut()
            .expect("source ref is object")
            .insert("detail".to_owned(), json!(detail));
    }
    value
}

fn decision_json(decision: &Decision) -> Value {
    json!({
        "id": decision.id,
        "choice": decision.choice,
        "authority": match decision.authority {
            DecisionAuthority::User => "user",
            DecisionAuthority::System => "system",
            DecisionAuthority::Organization => "organization",
        },
        "rationale": decision.rationale,
        "confidence": f64::from(decision.confidence_milli) / 1000.0,
        "reversibility": match decision.reversibility {
            Reversibility::Easy => "easy",
            Reversibility::Moderate => "moderate",
            Reversibility::Hard => "hard",
            Reversibility::Irreversible => "irreversible",
        },
        "source_refs": decision.source_refs.iter().map(source_ref_json).collect::<Vec<_>>(),
    })
}

fn unknown_json(unknown: &Unknown) -> Value {
    json!({
        "id": unknown.id,
        "question": unknown.question,
        "uncertainty": f64::from(unknown.uncertainty_milli) / 1000.0,
        "impact": f64::from(unknown.impact_milli) / 1000.0,
        "resolution": match unknown.resolution {
            UnknownResolution::AskUser => "ask_user",
            UnknownResolution::Research => "research",
            UnknownResolution::SystemDefault => "system_default",
            UnknownResolution::Defer => "defer",
        },
        "evidence_refs": unknown.evidence_refs.iter().map(source_ref_json).collect::<Vec<_>>(),
        "question_value": unknown.question_value(),
    })
}

fn build_delta(
    before: Option<&EngineeringIr>,
    after: &EngineeringIr,
    base_revision: u64,
    new_revision: u64,
    source_ref: SourceRef,
) -> SpecDelta {
    let before = before.map(ir_semantic_map).unwrap_or_default();
    let after_map = ir_semantic_map(after);
    let mut added_ids = Vec::new();
    let mut changed_ids = Vec::new();
    let mut invalidated_ids = Vec::new();

    for (id, statement) in &after_map {
        match before.get(id) {
            None => added_ids.push(id.clone()),
            Some(previous) if previous != statement => changed_ids.push(id.clone()),
            Some(_) => {}
        }
    }
    for id in before.keys() {
        if !after_map.contains_key(id) {
            invalidated_ids.push(id.clone());
        }
    }
    added_ids.sort();
    changed_ids.sort();
    invalidated_ids.sort();
    SpecDelta {
        base_revision,
        new_revision,
        source_ref,
        added_ids,
        changed_ids,
        invalidated_ids,
    }
}

fn ir_semantic_map(ir: &EngineeringIr) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for item in ir
        .goals
        .iter()
        .chain(ir.non_goals.iter())
        .chain(ir.constraints.iter())
        .chain(ir.quality_attributes.iter())
        .chain(ir.invariants.iter())
        .chain(ir.assumptions.iter())
        .chain(
            ir.acceptance_criteria
                .iter()
                .map(|criterion| &criterion.item),
        )
        .chain(
            ir.functional_requirements
                .iter()
                .map(|requirement| &requirement.item),
        )
    {
        map.insert(item.id.clone(), item.statement.clone());
    }
    for decision in &ir.decisions {
        map.insert(decision.id.clone(), decision.choice.clone());
    }
    for unknown in &ir.unknowns {
        map.insert(unknown.id.clone(), unknown.question.clone());
    }
    for finding in &ir.research_findings {
        map.insert(finding.claim_id.clone(), finding.statement.clone());
    }
    map
}

fn spec_delta_json(delta: &SpecDelta) -> Value {
    json!({
        "base_revision": delta.base_revision,
        "new_revision": delta.new_revision,
        "source_ref": source_ref_json(&delta.source_ref),
        "added_ids": delta.added_ids,
        "changed_ids": delta.changed_ids,
        "invalidated_ids": delta.invalidated_ids,
    })
}

fn latest_delta(events: &[StoredEvent]) -> Result<Option<SpecDelta>, SpecError> {
    let Some(event) = events
        .iter()
        .rev()
        .find(|event| event.event_type == "spec.delta.recorded")
    else {
        return Ok(None);
    };
    let value = inline_json(event)?;
    let source = value
        .get("source_ref")
        .and_then(Value::as_object)
        .ok_or_else(|| SpecError::Integrity("delta source_ref missing".to_owned()))?;
    let source_kind = match source.get("type").and_then(Value::as_str) {
        Some("user_message") => SourceKind::UserMessage,
        Some("research_claim") => SourceKind::ResearchClaim,
        Some("system_default") => SourceKind::SystemDefault,
        Some("repository") => SourceKind::Repository,
        Some("adr") => SourceKind::ArchitectureDecision,
        _ => {
            return Err(SpecError::Integrity(
                "delta source_ref type invalid".to_owned(),
            ));
        }
    };
    Ok(Some(SpecDelta {
        base_revision: value
            .get("base_revision")
            .and_then(Value::as_u64)
            .ok_or_else(|| SpecError::Integrity("delta base_revision missing".to_owned()))?,
        new_revision: value
            .get("new_revision")
            .and_then(Value::as_u64)
            .ok_or_else(|| SpecError::Integrity("delta new_revision missing".to_owned()))?,
        source_ref: SourceRef {
            kind: source_kind,
            id: source
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| SpecError::Integrity("delta source id missing".to_owned()))?
                .to_owned(),
            detail: source
                .get("detail")
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        added_ids: string_array(&value, "added_ids")?,
        changed_ids: string_array(&value, "changed_ids")?,
        invalidated_ids: string_array(&value, "invalidated_ids")?,
    }))
}

fn string_array(value: &Value, field: &str) -> Result<Vec<String>, SpecError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| SpecError::Integrity(format!("delta {field} missing")))?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| SpecError::Integrity(format!("delta {field} must contain strings")))
        })
        .collect()
}

fn empty_snapshot(project_id: &str) -> SpecSnapshot {
    SpecSnapshot {
        project_id: project_id.to_owned(),
        revision: 0,
        intent: IntentState::empty(),
        ir: None,
        checksum: None,
        research_artifact_count: 0,
        latest_delta: None,
    }
}

#[derive(Debug)]
pub enum SpecError {
    InvalidInput(&'static str),
    Contract(String),
    Semantic(Vec<String>),
    SemanticChecksum(SemanticChecksum),
    Integrity(String),
    LowerLayer {
        context: &'static str,
        message: String,
    },
    Storage(aer_storage::StorageError),
    Research(aer_research::ResearchError),
    Json(serde_json::Error),
}

impl fmt::Display for SpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid intent input: {message}"),
            Self::Contract(message) => write!(formatter, "spec contract validation: {message}"),
            Self::Semantic(issues) => {
                write!(formatter, "spec semantic validation: {}", issues.join("; "))
            }
            Self::SemanticChecksum(checksum) => write!(
                formatter,
                "semantic checksum blocked IR: missing={:?}, distorted={:?}, unsupported={:?}",
                checksum.missing, checksum.distorted, checksum.unsupported_additions
            ),
            Self::Integrity(message) => write!(formatter, "spec integrity: {message}"),
            Self::LowerLayer { context, message } => write!(formatter, "{context}: {message}"),
            Self::Storage(error) => error.fmt(formatter),
            Self::Research(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl Error for SpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Research(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidInput(_)
            | Self::Contract(_)
            | Self::Semantic(_)
            | Self::SemanticChecksum(_)
            | Self::Integrity(_)
            | Self::LowerLayer { .. } => None,
        }
    }
}

impl From<aer_storage::StorageError> for SpecError {
    fn from(error: aer_storage::StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<aer_research::ResearchError> for SpecError {
    fn from(error: aer_research::ResearchError) -> Self {
        Self::Research(error)
    }
}

impl From<serde_json::Error> for SpecError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command, time::SystemTime};

    use serde_json::json;

    use super::{SpecService, UserSemanticKind};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "everything-spec-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .expect("git spawn");
        assert!(status.success(), "git command failed: {args:?}");
    }

    fn repo() -> PathBuf {
        let repo = temp_dir("repo");
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        git(&repo, &["config", "user.name", "everything fixture"]);
        fs::write(repo.join("README.md"), "fixture\n").expect("readme");
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-q", "-m", "fixture"]);
        repo
    }

    #[test]
    fn greenfield_prompt_compiles_stable_source_backed_ir_and_question() {
        let repo = repo();
        let state = temp_dir("state");
        let first = SpecService::submit_message(
            &repo,
            &state,
            "Build a deterministic local engineering assistant.",
        )
        .expect("first compile");
        assert_eq!(first.revision, 1);
        let ir = first.ir.as_ref().expect("IR");
        assert_eq!(ir.goals.len(), 1);
        assert_eq!(
            ir.goals[0].statement,
            "Build a deterministic local engineering assistant."
        );
        assert_eq!(first.open_unknown_count(), 1);
        assert!(first.next_question().is_some());
        assert_eq!(first.checksum.as_ref().expect("checksum").missing.len(), 0);

        let inspected = SpecService::inspect(&repo, &state).expect("inspect");
        assert_eq!(inspected.revision, 1);
        assert_eq!(
            inspected.ir.as_ref().expect("IR").goals[0].id,
            ir.goals[0].id
        );
        fs::remove_dir_all(state).expect("state cleanup");
        fs::remove_dir_all(repo).expect("repo cleanup");
    }

    #[test]
    fn explicit_acceptance_resolves_high_value_unknown_and_creates_delta() {
        let repo = repo();
        let state = temp_dir("accept-state");
        SpecService::submit_message(&repo, &state, "Build the product.").expect("intent");
        let snapshot = SpecService::record_semantic(
            &repo,
            &state,
            UserSemanticKind::AcceptanceCriterion,
            "The canonical verification script exits successfully.",
        )
        .expect("acceptance");
        assert_eq!(snapshot.revision, 2);
        assert_eq!(snapshot.open_unknown_count(), 0);
        assert_eq!(
            snapshot.ir.as_ref().expect("IR").acceptance_criteria.len(),
            1
        );
        assert!(
            snapshot
                .latest_delta
                .as_ref()
                .expect("delta")
                .added_ids
                .iter()
                .any(|id| id.starts_with("AC-"))
        );
        fs::remove_dir_all(state).expect("state cleanup");
        fs::remove_dir_all(repo).expect("repo cleanup");
    }

    #[test]
    fn research_is_preserved_as_external_finding_and_cannot_self_promote() {
        let repo = repo();
        let state = temp_dir("research-state");
        SpecService::submit_message(&repo, &state, "Build the product.").expect("intent");
        let artifact = json!({
            "schema_version": 1,
            "research_id": "RES-1",
            "question": "Does an external source recommend X?",
            "observed_at": "2026-08-16T00:00:00Z",
            "sources": [{
                "source_id": "SRC-1",
                "uri": "https://example.invalid/spec",
                "source_class": "official",
                "retrieved_at": "2026-08-16T00:00:00Z",
                "content_hash": "sha256:abc"
            }],
            "claims": [{
                "claim_id": "CLM-1",
                "statement": "Use architecture X.",
                "source_refs": ["SRC-1"],
                "confidence": 0.9,
                "status": "supported"
            }]
        });
        let snapshot = SpecService::ingest_research(&repo, &state, artifact).expect("research");
        let ir = snapshot.ir.as_ref().expect("IR");
        assert_eq!(ir.research_findings.len(), 1);
        assert!(
            ir.decisions.is_empty(),
            "research self-promoted to decision"
        );
        assert!(
            ir.functional_requirements.is_empty(),
            "research self-promoted to requirement"
        );
        fs::remove_dir_all(state).expect("state cleanup");
        fs::remove_dir_all(repo).expect("repo cleanup");
    }
}
