use std::{fs, path::Path};

use aer_domain::compatibility::DATABASE_SCHEMA_VERSION;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};

use crate::{Result, StorageError, object_store::sha256_hex};

pub(crate) const AER_SQLITE_APPLICATION_ID: i32 = 0x4145_5231;
const MIGRATION_V1_ID: &str = "0001_durable_state_kernel";

pub(crate) const MIGRATION_V1_SQL: &str = r#"
PRAGMA application_id = 1095062065;
PRAGMA user_version = 1;

CREATE TABLE aer_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE migration_history (
    schema_version INTEGER PRIMARY KEY CHECK (schema_version > 0),
    migration_id TEXT NOT NULL UNIQUE,
    checksum TEXT NOT NULL CHECK (length(checksum) = 64),
    applied_at TEXT NOT NULL
) STRICT;

CREATE TABLE object_blobs (
    hash TEXT PRIMARY KEY CHECK (length(hash) = 64),
    byte_len INTEGER NOT NULL CHECK (byte_len >= 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE object_scopes (
    hash TEXT NOT NULL,
    project_id TEXT NOT NULL CHECK (length(project_id) > 0),
    sensitivity TEXT NOT NULL CHECK (
        sensitivity IN ('public', 'internal', 'confidential', 'restricted')
    ),
    retention_class TEXT NOT NULL CHECK (length(retention_class) > 0),
    expires_at TEXT,
    pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    PRIMARY KEY (hash, project_id),
    FOREIGN KEY (hash) REFERENCES object_blobs(hash) ON DELETE RESTRICT
) STRICT;

CREATE TABLE events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE CHECK (length(event_id) > 0),
    project_id TEXT NOT NULL CHECK (length(project_id) > 0),
    run_id TEXT,
    task_id TEXT,
    event_type TEXT NOT NULL CHECK (length(event_type) > 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    timestamp TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    payload_json TEXT CHECK (payload_json IS NULL OR json_valid(payload_json)),
    payload_artifact_hash TEXT,
    causation_id TEXT,
    correlation_id TEXT,
    CHECK (NOT (payload_json IS NOT NULL AND payload_artifact_hash IS NOT NULL)),
    FOREIGN KEY (payload_artifact_hash, project_id)
        REFERENCES object_scopes(hash, project_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX events_project_sequence_idx ON events(project_id, sequence);
CREATE INDEX events_run_sequence_idx ON events(run_id, sequence) WHERE run_id IS NOT NULL;
CREATE INDEX events_causation_idx ON events(causation_id) WHERE causation_id IS NOT NULL;

CREATE TABLE projection_heads (
    project_id TEXT PRIMARY KEY,
    event_count INTEGER NOT NULL CHECK (event_count > 0),
    last_sequence INTEGER NOT NULL CHECK (last_sequence > 0),
    last_event_id TEXT NOT NULL,
    rolling_digest TEXT NOT NULL CHECK (length(rolling_digest) = 64)
) STRICT;

CREATE TRIGGER events_reject_update
BEFORE UPDATE ON events
BEGIN
    SELECT RAISE(ABORT, 'AER event history is immutable');
END;

CREATE TRIGGER events_reject_delete
BEFORE DELETE ON events
BEGIN
    SELECT RAISE(ABORT, 'AER event history is immutable');
END;

CREATE TRIGGER migration_history_reject_update
BEFORE UPDATE ON migration_history
BEGIN
    SELECT RAISE(ABORT, 'AER migration history is immutable');
END;

CREATE TRIGGER migration_history_reject_delete
BEFORE DELETE ON migration_history
BEGIN
    SELECT RAISE(ABORT, 'AER migration history is immutable');
END;
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Preflight {
    pub version: u32,
    pub fresh: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MigrationFault {
    None,
    BeforeCommit,
}

pub(crate) fn inspect(path: &Path) -> Result<Preflight> {
    if !path.exists() {
        return Ok(Preflight {
            version: 0,
            fresh: true,
        });
    }

    let metadata = fs::metadata(path).map_err(|source| StorageError::io(path, source))?;
    if metadata.len() == 0 {
        return Ok(Preflight {
            version: 0,
            fresh: true,
        });
    }

    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != i64::from(AER_SQLITE_APPLICATION_ID) {
        return Err(StorageError::UnrecognizedDatabase(path.to_path_buf()));
    }

    let has_meta: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'aer_meta')",
        [],
        |row| row.get(0),
    )?;
    if !has_meta {
        return Err(StorageError::UnrecognizedDatabase(path.to_path_buf()));
    }

    let version_text: Option<String> = connection
        .query_row(
            "SELECT value FROM aer_meta WHERE key = 'database_schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let version_text =
        version_text.ok_or_else(|| StorageError::UnrecognizedDatabase(path.to_path_buf()))?;
    let version = version_text
        .parse::<u32>()
        .map_err(|_| StorageError::InvalidDatabaseVersion(version_text.clone()))?;

    let user_version_raw: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let user_version = u32::try_from(user_version_raw)
        .map_err(|_| StorageError::InvalidDatabaseVersion(user_version_raw.to_string()))?;
    if user_version != version {
        return Err(StorageError::MigrationInvariant(format!(
            "PRAGMA user_version {user_version} does not match aer_meta version {version}"
        )));
    }

    if version > DATABASE_SCHEMA_VERSION {
        return Err(StorageError::UnsupportedDatabaseVersion {
            found: version,
            supported: DATABASE_SCHEMA_VERSION,
        });
    }

    Ok(Preflight {
        version,
        fresh: false,
    })
}

pub(crate) fn migrate(
    connection: &mut Connection,
    preflight: Preflight,
    fault: MigrationFault,
) -> Result<()> {
    match preflight.version {
        DATABASE_SCHEMA_VERSION => verify_baseline(connection),
        0 if preflight.fresh => migrate_fresh_to_v1(connection, fault),
        version => Err(StorageError::MigrationInvariant(format!(
            "no migration path from database schema version {version} to {DATABASE_SCHEMA_VERSION}"
        ))),
    }
}

fn migrate_fresh_to_v1(connection: &mut Connection, fault: MigrationFault) -> Result<()> {
    let checksum = sha256_hex(MIGRATION_V1_SQL.as_bytes());
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(MIGRATION_V1_SQL)?;
    transaction.execute(
        "INSERT INTO aer_meta(key, value) VALUES ('database_schema_version', ?1)",
        [DATABASE_SCHEMA_VERSION.to_string()],
    )?;
    transaction.execute(
        "INSERT INTO migration_history(schema_version, migration_id, checksum, applied_at)
         VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        params![
            i64::from(DATABASE_SCHEMA_VERSION),
            MIGRATION_V1_ID,
            checksum
        ],
    )?;

    if fault == MigrationFault::BeforeCommit {
        return Err(StorageError::FaultInjected("migration_before_commit"));
    }

    transaction.commit()?;
    verify_baseline(connection)
}

pub(crate) fn verify_baseline(connection: &Connection) -> Result<()> {
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    if application_id != i64::from(AER_SQLITE_APPLICATION_ID) {
        return Err(StorageError::MigrationInvariant(
            "SQLite application_id does not identify AER durable state".to_owned(),
        ));
    }

    let version_text: String = connection.query_row(
        "SELECT value FROM aer_meta WHERE key = 'database_schema_version'",
        [],
        |row| row.get(0),
    )?;
    let version = version_text
        .parse::<u32>()
        .map_err(|_| StorageError::InvalidDatabaseVersion(version_text.clone()))?;
    if version != DATABASE_SCHEMA_VERSION {
        return Err(StorageError::MigrationInvariant(format!(
            "database schema version is {version}; expected {DATABASE_SCHEMA_VERSION}"
        )));
    }

    let user_version_raw: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let user_version = u32::try_from(user_version_raw)
        .map_err(|_| StorageError::InvalidDatabaseVersion(user_version_raw.to_string()))?;
    if user_version != DATABASE_SCHEMA_VERSION {
        return Err(StorageError::MigrationInvariant(format!(
            "PRAGMA user_version is {user_version}; expected {DATABASE_SCHEMA_VERSION}"
        )));
    }

    let expected_checksum = sha256_hex(MIGRATION_V1_SQL.as_bytes());
    let stored: (String, String) = connection.query_row(
        "SELECT migration_id, checksum FROM migration_history WHERE schema_version = ?1",
        [i64::from(DATABASE_SCHEMA_VERSION)],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if stored.0 != MIGRATION_V1_ID || stored.1 != expected_checksum {
        return Err(StorageError::MigrationInvariant(
            "baseline migration identity/checksum drifted from the checked-in implementation"
                .to_owned(),
        ));
    }

    for table in [
        "aer_meta",
        "migration_history",
        "object_blobs",
        "object_scopes",
        "events",
        "projection_heads",
    ] {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(StorageError::MigrationInvariant(format!(
                "required durable table {table} is missing"
            )));
        }
    }

    Ok(())
}
