use std::{
    fs,
    path::{Path, PathBuf},
};

use aer_domain::compatibility::{DATABASE_SCHEMA_VERSION, EVENT_SCHEMA_VERSION};
use rusqlite::Connection;
use serde_json::json;
use ulid::Ulid;

use crate::{
    Causation, DurableState, EventPayload, NewEvent, ObjectHash, ObjectMetadata, Sensitivity,
    StorageError,
    state::{application_id, object_file_path},
};

struct TestDir(PathBuf);

impl TestDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("aer-storage-{label}-{}", Ulid::generate()));
        fs::create_dir_all(&path).expect("test directory should be creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn metadata() -> ObjectMetadata {
    ObjectMetadata::new(Sensitivity::Internal, "project-default")
}

#[test]
fn fresh_store_uses_wal_full_and_activates_database_v1() {
    let directory = TestDir::new("durability");
    let state = DurableState::open(directory.path()).expect("fresh durable state should open");
    assert_eq!(
        state
            .database_schema_version()
            .expect("schema version should read"),
        DATABASE_SCHEMA_VERSION
    );
    let diagnostics = state
        .durability_diagnostics()
        .expect("durability pragmas should be inspectable");
    assert_eq!(diagnostics.journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(diagnostics.synchronous, 2);
    assert!(diagnostics.foreign_keys);
}

#[test]
fn future_database_version_fails_before_migration_mutation() {
    let directory = TestDir::new("future-version");
    let paths = crate::StoragePaths::for_workspace(directory.path());
    fs::create_dir_all(paths.state_root()).expect("state root should be creatable");
    let connection = Connection::open(paths.database()).expect("fixture database should open");
    connection
        .execute_batch(&format!(
            "PRAGMA application_id = {};
             PRAGMA user_version = 2;
             CREATE TABLE aer_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
             INSERT INTO aer_meta(key, value) VALUES ('database_schema_version', '2');",
            application_id()
        ))
        .expect("future fixture should be initialized");
    drop(connection);

    assert!(matches!(
        DurableState::open(directory.path()),
        Err(StorageError::UnsupportedDatabaseVersion { found: 2, .. })
    ));

    let connection = Connection::open(paths.database()).expect("fixture should remain readable");
    let migration_table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'migration_history')",
            [],
            |row| row.get(0),
        )
        .expect("fixture should remain queryable");
    assert!(!migration_table_exists);
}

#[test]
fn unrelated_existing_sqlite_database_is_not_claimed_as_aer_state() {
    let directory = TestDir::new("foreign-db");
    let paths = crate::StoragePaths::for_workspace(directory.path());
    fs::create_dir_all(paths.state_root()).expect("state root should be creatable");
    let connection = Connection::open(paths.database()).expect("fixture database should open");
    connection
        .execute("CREATE TABLE user_data(value TEXT)", [])
        .expect("fixture table should be creatable");
    drop(connection);

    assert!(matches!(
        DurableState::open(directory.path()),
        Err(StorageError::UnrecognizedDatabase(_))
    ));
}

#[test]
fn unrelated_existing_aer_directory_is_not_claimed() {
    let directory = TestDir::new("foreign-state-dir");
    let state_root = directory.path().join(".aer");
    fs::create_dir_all(&state_root).expect("fixture state root should be creatable");
    fs::write(state_root.join("other-tool.txt"), b"not AER")
        .expect("foreign fixture should be writable");

    assert!(matches!(
        DurableState::open(directory.path()),
        Err(StorageError::UnrecognizedStateDirectory(_))
    ));
    assert!(!state_root.join("state.db").exists());
}

#[test]
fn migration_failure_rolls_back_and_fresh_state_can_retry_cleanly() {
    let directory = TestDir::new("migration-crash");
    assert!(matches!(
        DurableState::open_with_migration_fault(directory.path()),
        Err(StorageError::FaultInjected("migration_before_commit"))
    ));

    let paths = crate::StoragePaths::for_workspace(directory.path());
    assert!(!paths.database().exists());

    let state = DurableState::open(directory.path()).expect("clean retry should succeed");
    assert_eq!(
        state
            .database_schema_version()
            .expect("version should be readable"),
        DATABASE_SCHEMA_VERSION
    );
}

#[test]
fn object_identity_is_sha256_idempotent_and_secret_data_is_rejected() {
    let directory = TestDir::new("objects");
    let mut state = DurableState::open(directory.path()).expect("state should open");
    let bytes = b"durable artifact bytes";
    let first = state
        .put_object("project-a", bytes, &metadata())
        .expect("object should persist");
    let second = state
        .put_object("project-a", bytes, &metadata())
        .expect("same object should be idempotent");
    assert_eq!(first, second);
    assert_eq!(
        state
            .read_object("project-a", &first)
            .expect("object should verify"),
        bytes
    );

    let secret = ObjectMetadata::new(Sensitivity::Secret, "credential");
    assert!(matches!(
        state.put_object("project-a", b"api-key", &secret),
        Err(StorageError::SecretObjectRejected)
    ));
}

#[test]
fn object_file_first_failure_leaves_recoverable_orphan_not_authoritative_reference() {
    let directory = TestDir::new("object-fault");
    let mut state = DurableState::open(directory.path()).expect("state should open");
    let bytes = b"recoverable orphan";
    let hash = ObjectHash::of_bytes(bytes);

    assert!(matches!(
        state.put_object_with_file_fault("project-a", bytes, &metadata()),
        Err(StorageError::FaultInjected("object_after_file_sync"))
    ));
    assert!(object_file_path(state.paths(), &hash).is_file());

    let event = NewEvent {
        payload: EventPayload::Artifact(hash.clone()),
        ..NewEvent::new("project-a", "artifact.created")
    };
    assert!(matches!(
        state.append_event(event),
        Err(StorageError::ArtifactNotScoped { .. })
    ));

    let registered = state
        .put_object("project-a", bytes, &metadata())
        .expect("retry should register the already durable bytes");
    assert_eq!(registered, hash);
}

#[test]
fn artifact_reference_is_project_scoped() {
    let directory = TestDir::new("artifact-scope");
    let mut state = DurableState::open(directory.path()).expect("state should open");
    let hash = state
        .put_object("project-a", b"scoped", &metadata())
        .expect("object should persist");

    assert!(matches!(
        state.read_object("project-b", &hash),
        Err(StorageError::ArtifactNotScoped { .. })
    ));
    let foreign = NewEvent {
        payload: EventPayload::Artifact(hash),
        ..NewEvent::new("project-b", "artifact.used")
    };
    assert!(matches!(
        state.append_event(foreign),
        Err(StorageError::ArtifactNotScoped { .. })
    ));
}

#[test]
fn event_ids_and_database_sequences_are_strictly_increasing() {
    let directory = TestDir::new("event-order");
    let mut state = DurableState::open(directory.path()).expect("state should open");

    let first = state
        .append_event(NewEvent::new("project-a", "project.created"))
        .expect("first event should append");
    let second = state
        .append_event(NewEvent::new("project-a", "project.updated"))
        .expect("second event should append");

    assert!(first.sequence < second.sequence);
    assert!(first.event_id < second.event_id);
    assert_eq!(first.schema_version, EVENT_SCHEMA_VERSION);
    assert_ne!(first.event_id, second.event_id);
}

#[test]
fn causation_must_resolve_unless_explicitly_external() {
    let directory = TestDir::new("causation");
    let mut state = DurableState::open(directory.path()).expect("state should open");

    let dangling = NewEvent {
        causation: Some(Causation::Event("01UNKNOWN".to_owned())),
        ..NewEvent::new("project-a", "task.created")
    };
    assert!(matches!(
        state.append_event(dangling),
        Err(StorageError::DanglingCausation(_))
    ));

    let external = NewEvent {
        causation: Some(Causation::External("user-command-1".to_owned())),
        ..NewEvent::new("project-a", "project.created")
    };
    let external = state
        .append_event(external)
        .expect("explicit external causation should append");
    assert_eq!(
        external.causation_id.as_deref(),
        Some("external:user-command-1")
    );

    let child = NewEvent {
        causation: Some(Causation::Event(external.event_id.clone())),
        payload: EventPayload::Inline(json!({"source": "test"})),
        ..NewEvent::new("project-a", "task.created")
    };
    state
        .append_event(child)
        .expect("journal causation should resolve");

    let other_project = NewEvent {
        causation: Some(Causation::Event(external.event_id)),
        ..NewEvent::new("project-b", "task.created")
    };
    assert!(matches!(
        state.append_event(other_project),
        Err(StorageError::CausationScopeMismatch { .. })
    ));
}

#[test]
fn event_and_projection_commit_atomically() {
    let directory = TestDir::new("event-transaction");
    let mut state = DurableState::open(directory.path()).expect("state should open");
    let initial = state
        .append_event(NewEvent::new("project-a", "project.created"))
        .expect("initial event should append");

    assert!(matches!(
        state.append_event_with_transaction_fault(NewEvent::new("project-a", "task.created")),
        Err(StorageError::FaultInjected("event_after_insert"))
    ));

    let events = state.events("project-a").expect("events should read");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, initial.event_id);
    let projection = state
        .verify_projection("project-a")
        .expect("projection should remain equivalent after rollback");
    assert_eq!(projection.event_count, 1);
}

#[test]
fn event_history_rejects_update_and_delete() {
    let directory = TestDir::new("immutable-events");
    let mut state = DurableState::open(directory.path()).expect("state should open");
    let event = state
        .append_event(NewEvent::new("project-a", "project.created"))
        .expect("event should append");
    let database = state.paths().database().to_path_buf();
    drop(state);

    let connection = Connection::open(database).expect("database should reopen");
    assert!(
        connection
            .execute(
                "UPDATE events SET event_type = 'tampered' WHERE event_id = ?1",
                [&event.event_id],
            )
            .is_err()
    );
    assert!(
        connection
            .execute("DELETE FROM events WHERE event_id = ?1", [&event.event_id])
            .is_err()
    );
}

#[test]
fn replay_detects_projection_drift_and_can_rebuild() {
    let directory = TestDir::new("replay");
    let mut state = DurableState::open(directory.path()).expect("state should open");
    for event_type in ["project.created", "task.created", "task.ready"] {
        state
            .append_event(NewEvent::new("project-a", event_type))
            .expect("event should append");
    }

    let expected = state
        .verify_projection("project-a")
        .expect("projection should initially verify");
    let database = state.paths().database().to_path_buf();
    let connection = Connection::open(database).expect("second connection should open");
    connection
        .execute(
            "UPDATE projection_heads SET rolling_digest = ?1 WHERE project_id = 'project-a'",
            ["0000000000000000000000000000000000000000000000000000000000000000"],
        )
        .expect("projection fixture should be corruptible");
    drop(connection);

    assert!(matches!(
        state.verify_projection("project-a"),
        Err(StorageError::ProjectionMismatch { .. })
    ));
    let rebuilt = state
        .rebuild_projection("project-a")
        .expect("projection should rebuild from immutable events");
    assert_eq!(rebuilt, expected);
    assert_eq!(
        state
            .verify_projection("project-a")
            .expect("rebuilt projection should verify"),
        expected
    );
}

#[test]
fn reopen_preserves_event_replay_object_integrity_and_migration_identity() {
    let directory = TestDir::new("reopen");
    let expected_projection;
    {
        let mut state = DurableState::open(directory.path()).expect("state should open");
        let hash = state
            .put_object("project-a", b"proof bytes", &metadata())
            .expect("object should persist");
        let event = NewEvent {
            payload: EventPayload::Artifact(hash),
            ..NewEvent::new("project-a", "evidence.created")
        };
        state.append_event(event).expect("event should append");
        expected_projection = state
            .verify_project_integrity("project-a")
            .expect("project should verify before close");
    }

    let reopened = DurableState::open(directory.path()).expect("state should reopen");
    assert_eq!(
        reopened
            .database_schema_version()
            .expect("version should remain readable"),
        DATABASE_SCHEMA_VERSION
    );
    assert_eq!(
        reopened
            .verify_project_integrity("project-a")
            .expect("reopened project should verify"),
        expected_projection
    );
}

#[test]
fn referenced_object_corruption_is_detected() {
    let directory = TestDir::new("object-corruption");
    let mut state = DurableState::open(directory.path()).expect("state should open");
    let hash = state
        .put_object("project-a", b"original", &metadata())
        .expect("object should persist");
    state
        .append_event(NewEvent {
            payload: EventPayload::Artifact(hash.clone()),
            ..NewEvent::new("project-a", "evidence.created")
        })
        .expect("event should append");

    fs::write(object_file_path(state.paths(), &hash), b"tampered")
        .expect("fixture should corrupt object bytes");
    assert!(matches!(
        state.verify_project_integrity("project-a"),
        Err(StorageError::ObjectCorrupt { .. })
    ));
}
