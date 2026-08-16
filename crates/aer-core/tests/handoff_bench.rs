use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use aer_core::engineering_state::{
    EngineeringMemoryRecord, EngineeringStateStore, FailureClass, HandoffPolicy, HandoffRequest,
    HypothesisState, InvalidationScope, MemoryKind, MemoryValidity, ProgressWindow, RecoveryAction,
    RecoveryBudget, RecoveryState, RepositoryEntityRef, StagnationPolicy, assess_stagnation,
};
use aer_repo::RepositoryChangeSet;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn temp_root(label: &str) -> PathBuf {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "aer-handoff-bench-{}-{label}-{serial}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temp root");
    path
}

fn record(
    id: &str,
    kind: MemoryKind,
    statement: &str,
    entity: Option<&str>,
) -> EngineeringMemoryRecord {
    EngineeringMemoryRecord {
        record_id: id.to_owned(),
        kind,
        statement: statement.to_owned(),
        validity: MemoryValidity::Current,
        confidence_milli: 1000,
        repo_snapshot: Some("repo-v1".to_owned()),
        spec_version: Some("spec-v1".to_owned()),
        evidence_refs: if kind == MemoryKind::VerifiedFact {
            vec![format!("evidence:{id}")]
        } else {
            Vec::new()
        },
        repository_entities: entity
            .map(|entity_id| RepositoryEntityRef {
                entity_id: entity_id.to_owned(),
                repo_snapshot: "repo-v1".to_owned(),
            })
            .into_iter()
            .collect(),
        invalidation_scope: InvalidationScope {
            repository_entity_ids: entity.into_iter().map(str::to_owned).collect(),
            ..InvalidationScope::default()
        },
        hypothesis_state: (kind == MemoryKind::Hypothesis).then_some(HypothesisState::Disproven),
        failure_class: (kind == MemoryKind::FailureFingerprint)
            .then_some(FailureClass::ImplementationError),
        supersedes: None,
    }
}

fn handoff_request() -> HandoffRequest {
    HandoffRequest {
        task_id: "task-auth".to_owned(),
        objective: "Fix authentication behavior without rediscovering disproven paths".to_owned(),
        spec_version: "spec-v1".to_owned(),
        repo_snapshot: "repo-v1".to_owned(),
        requested_action: "Continue from verified state and run focused verifier".to_owned(),
        evidence_refs: vec!["evidence:fact-auth".to_owned()],
        relevant_context_refs: vec!["context:auth-service".to_owned()],
        unresolved_dependencies: vec!["dependency:identity-provider".to_owned()],
    }
}

#[test]
fn handoff_bench_compacts_verified_state_without_transcript_bloat() {
    let root = temp_root("compaction");
    let mut store = EngineeringStateStore::open(&root, "project-handoff").expect("open store");
    store
        .record(&record(
            "fact-auth",
            MemoryKind::VerifiedFact,
            "AuthService rejects expired sessions before provider access.",
            Some("file:auth-service"),
        ))
        .expect("record fact");
    store
        .record(&record(
            "hyp-redis",
            MemoryKind::Hypothesis,
            "Redis replay was tested and disproven as the duplicate-session cause.",
            Some("file:auth-service"),
        ))
        .expect("record hypothesis");
    store
        .record(&record(
            "failure-old-path",
            MemoryKind::FailureFingerprint,
            "Blindly retrying the same authentication provider repeated the failure.",
            Some("file:auth-service"),
        ))
        .expect("record failure");

    let handoff = store
        .compact_handoff(
            &handoff_request(),
            HandoffPolicy {
                max_records: 8,
                max_bytes: 2_048,
            },
        )
        .expect("compact handoff");

    let raw_trajectory =
        "read auth.rs; rerun test; inspect trace; reconsider Redis; rediscover expiry behavior; "
            .repeat(120);
    assert!(
        handoff
            .records
            .iter()
            .any(|record| record.record_id == "fact-auth")
    );
    assert!(handoff.records.iter().any(|record| {
        record.record_id == "hyp-redis"
            && record.hypothesis_state == Some(HypothesisState::Disproven)
    }));
    assert!(handoff.estimated_tokens < raw_trajectory.len().div_ceil(4));
    assert!(!handoff.handoff_id.is_empty());
    store.verify_integrity().expect("journal integrity");
    drop(store);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn handoff_bench_repository_change_invalidates_only_linked_current_memory() {
    let root = temp_root("invalidation");
    let mut store = EngineeringStateStore::open(&root, "project-invalidation").expect("open store");
    store
        .record(&record(
            "fact-auth",
            MemoryKind::VerifiedFact,
            "AuthService currently validates expiry.",
            Some("file:auth-service"),
        ))
        .expect("record fact");
    store
        .record(&record(
            "user-decision",
            MemoryKind::UserDecision,
            "Keep the public authentication contract stable.",
            None,
        ))
        .expect("record decision");

    let invalidated = store
        .invalidate_repository_changes(&RepositoryChangeSet {
            from_snapshot: "repo-v1".to_owned(),
            to_snapshot: "repo-v2".to_owned(),
            added_paths: Vec::new(),
            changed_paths: vec!["src/auth.rs".to_owned()],
            deleted_paths: Vec::new(),
            invalidated_entity_ids: vec!["file:auth-service".to_owned()],
        })
        .expect("invalidate");
    assert_eq!(invalidated, vec!["fact-auth".to_owned()]);

    let projection = store.projection().expect("projection");
    assert_eq!(
        projection
            .records
            .iter()
            .find(|record| record.record_id == "fact-auth")
            .expect("fact")
            .validity,
        MemoryValidity::Invalidated
    );
    assert_eq!(
        projection
            .records
            .iter()
            .find(|record| record.record_id == "user-decision")
            .expect("decision")
            .validity,
        MemoryValidity::Current
    );
    let handoff = store
        .compact_handoff(&handoff_request(), HandoffPolicy::default())
        .expect("handoff");
    assert!(
        !handoff
            .records
            .iter()
            .any(|record| record.record_id == "fact-auth")
    );
    assert!(
        handoff
            .records
            .iter()
            .any(|record| record.record_id == "user-decision")
    );
    drop(store);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn handoff_bench_stagnation_and_recovery_are_calibrated_and_bounded() {
    let policy = StagnationPolicy {
        repetition_threshold: 6,
        max_new_evidence: 1,
        max_new_entities: 1,
        flat_verifier_delta_milli: 5,
    };
    let assessment = assess_stagnation(
        ProgressWindow {
            repeated_commands: 3,
            repeated_file_reads: 2,
            edit_reverts: 1,
            new_evidence: 0,
            new_relevant_entities: 0,
            verifier_improvement_milli: 0,
            accepted_subgoals: 0,
        },
        policy,
    )
    .expect("assessment");
    assert!(assessment.stagnant);

    let budget = RecoveryBudget {
        maximum_escalations: 4,
    };
    let mut state = RecoveryState {
        escalations_used: 0,
    };
    let mut actions = Vec::new();
    while let Some(action) = state.next(budget) {
        actions.push(action);
        state = state.advance(budget).expect("advance");
    }
    assert_eq!(
        actions,
        vec![
            RecoveryAction::ToolRetry,
            RecoveryAction::ContextRefresh,
            RecoveryAction::HypothesisReset,
            RecoveryAction::FreshContextTakeover,
        ]
    );
}
