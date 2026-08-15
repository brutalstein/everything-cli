use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, TransactionBehavior, params};
use ulid::Generator;

use crate::{
    DurabilityDiagnostics, ObjectHash, ObjectMetadata, Result, Sensitivity, StorageError,
    StoragePaths,
    migration::{self, MigrationFault},
    object_store::{object_path, persist_bytes_atomically, read_verified},
};

/// Single-coordinator durable-state kernel.
pub struct DurableState {
    pub(crate) connection: Connection,
    pub(crate) paths: StoragePaths,
    pub(crate) event_ids: Generator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObjectFault {
    None,
    AfterFileSync,
}

impl DurableState {
    /// Opens or creates `.aer/state.db` after fail-closed compatibility preflight.
    pub fn open(workspace_root: impl AsRef<Path>) -> Result<Self> {
        Self::open_internal(workspace_root.as_ref(), MigrationFault::None)
    }

    fn open_internal(workspace_root: &Path, migration_fault: MigrationFault) -> Result<Self> {
        let paths = StoragePaths::for_workspace(workspace_root);
        let preflight = migration::inspect(paths.database())?;
        if preflight.fresh {
            verify_fresh_layout_is_claimable(&paths)?;
        }
        create_layout(&paths)?;

        let mut connection = Connection::open(paths.database())?;
        if !preflight.fresh
            && let Err(error) = migration::verify_baseline(&connection)
        {
            drop(connection);
            return Err(error);
        }
        if let Err(error) = configure_connection(&connection) {
            drop(connection);
            if preflight.fresh {
                cleanup_fresh_database(paths.database());
            }
            return Err(error);
        }
        if let Err(error) = migration::migrate(&mut connection, preflight, migration_fault) {
            drop(connection);
            if preflight.fresh {
                cleanup_fresh_database(paths.database());
            }
            return Err(error);
        }

        Ok(Self {
            connection,
            paths,
            event_ids: Generator::new(),
        })
    }

    #[must_use]
    pub fn paths(&self) -> &StoragePaths {
        &self.paths
    }

    pub fn database_schema_version(&self) -> Result<u32> {
        let value: String = self.connection.query_row(
            "SELECT value FROM aer_meta WHERE key = 'database_schema_version'",
            [],
            |row| row.get(0),
        )?;
        value
            .parse::<u32>()
            .map_err(|_| StorageError::InvalidDatabaseVersion(value))
    }

    pub fn durability_diagnostics(&self) -> Result<DurabilityDiagnostics> {
        let journal_mode: String =
            self.connection
                .pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        let synchronous: i64 = self
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))?;
        let foreign_keys: i64 =
            self.connection
                .pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
        Ok(DurabilityDiagnostics {
            journal_mode,
            synchronous,
            foreign_keys: foreign_keys == 1,
        })
    }

    /// Writes bytes before registering metadata. A crash can therefore leave an
    /// unreferenced file, but never a committed event pointing at missing bytes.
    pub fn put_object(
        &mut self,
        project_id: &str,
        bytes: &[u8],
        metadata: &ObjectMetadata,
    ) -> Result<ObjectHash> {
        self.put_object_internal(project_id, bytes, metadata, ObjectFault::None)
    }

    fn put_object_internal(
        &mut self,
        project_id: &str,
        bytes: &[u8],
        metadata: &ObjectMetadata,
        fault: ObjectFault,
    ) -> Result<ObjectHash> {
        validate_identifier("project_id", project_id)?;
        if metadata.retention_class.trim().is_empty() {
            return Err(StorageError::InvalidIdentifier {
                field: "retention_class",
            });
        }
        if let Some(expires_at) = metadata.expires_at.as_deref() {
            validate_identifier("expires_at", expires_at)?;
        }
        if metadata.sensitivity == Sensitivity::Secret {
            return Err(StorageError::SecretObjectRejected);
        }

        let hash = persist_bytes_atomically(self.paths.objects(), self.paths.tmp(), bytes)?;
        if fault == ObjectFault::AfterFileSync {
            return Err(StorageError::FaultInjected("object_after_file_sync"));
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO object_blobs(hash, byte_len) VALUES (?1, ?2)
             ON CONFLICT(hash) DO NOTHING",
            params![hash.as_str(), bytes.len() as i64],
        )?;
        let existing_len: i64 = transaction.query_row(
            "SELECT byte_len FROM object_blobs WHERE hash = ?1",
            [hash.as_str()],
            |row| row.get(0),
        )?;
        if existing_len != bytes.len() as i64 {
            return Err(StorageError::ObjectCorrupt {
                expected: hash.to_string(),
                actual: format!("registered byte_len={existing_len}"),
            });
        }

        transaction.execute(
            "INSERT INTO object_scopes(
                hash, project_id, sensitivity, retention_class, expires_at, pinned
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(hash, project_id) DO NOTHING",
            params![
                hash.as_str(),
                project_id,
                metadata.sensitivity.as_str(),
                metadata.retention_class.as_str(),
                metadata.expires_at.as_deref(),
                i64::from(u8::from(metadata.pinned)),
            ],
        )?;
        let existing: (String, String, Option<String>, i64) = transaction.query_row(
            "SELECT sensitivity, retention_class, expires_at, pinned
             FROM object_scopes WHERE hash = ?1 AND project_id = ?2",
            params![hash.as_str(), project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let expected = (
            metadata.sensitivity.as_str(),
            metadata.retention_class.as_str(),
            metadata.expires_at.as_deref(),
            i64::from(u8::from(metadata.pinned)),
        );
        if (
            existing.0.as_str(),
            existing.1.as_str(),
            existing.2.as_deref(),
            existing.3,
        ) != expected
        {
            return Err(StorageError::ObjectMetadataConflict {
                hash: hash.to_string(),
                project_id: project_id.to_owned(),
            });
        }
        transaction.commit()?;
        Ok(hash)
    }

    pub fn read_object(&self, project_id: &str, hash: &ObjectHash) -> Result<Vec<u8>> {
        validate_identifier("project_id", project_id)?;
        let scoped: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM object_scopes WHERE hash = ?1 AND project_id = ?2)",
            params![hash.as_str(), project_id],
            |row| row.get(0),
        )?;
        if !scoped {
            return Err(StorageError::ArtifactNotScoped {
                hash: hash.to_string(),
                project_id: project_id.to_owned(),
            });
        }
        read_verified(self.paths.objects(), hash)
    }

    #[cfg(test)]
    pub(crate) fn open_with_migration_fault(workspace_root: &Path) -> Result<Self> {
        Self::open_internal(workspace_root, MigrationFault::BeforeCommit)
    }

    #[cfg(test)]
    pub(crate) fn put_object_with_file_fault(
        &mut self,
        project_id: &str,
        bytes: &[u8],
        metadata: &ObjectMetadata,
    ) -> Result<ObjectHash> {
        self.put_object_internal(project_id, bytes, metadata, ObjectFault::AfterFileSync)
    }
}

fn create_layout(paths: &StoragePaths) -> Result<()> {
    for path in [
        paths.state_root(),
        paths.objects(),
        paths.tmp(),
        paths.backups(),
    ] {
        fs::create_dir_all(path).map_err(|source| StorageError::io(path, source))?;
    }
    Ok(())
}

fn verify_fresh_layout_is_claimable(paths: &StoragePaths) -> Result<()> {
    if !paths.state_root().exists() {
        return Ok(());
    }
    let entries = fs::read_dir(paths.state_root())
        .map_err(|source| StorageError::io(paths.state_root(), source))?;
    for entry in entries {
        let entry = entry.map_err(|source| StorageError::io(paths.state_root(), source))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        match name.as_ref() {
            "state.db" => {
                let metadata = entry
                    .metadata()
                    .map_err(|source| StorageError::io(entry.path(), source))?;
                if !metadata.is_file() || metadata.len() != 0 {
                    return Err(StorageError::UnrecognizedStateDirectory(
                        paths.state_root().to_path_buf(),
                    ));
                }
            }
            "objects" | "tmp" | "backups" => {
                let metadata = entry
                    .metadata()
                    .map_err(|source| StorageError::io(entry.path(), source))?;
                if !metadata.is_dir()
                    || fs::read_dir(entry.path())
                        .map_err(|source| StorageError::io(entry.path(), source))?
                        .next()
                        .is_some()
                {
                    return Err(StorageError::UnrecognizedStateDirectory(
                        paths.state_root().to_path_buf(),
                    ));
                }
            }
            _ => {
                return Err(StorageError::UnrecognizedStateDirectory(
                    paths.state_root().to_path_buf(),
                ));
            }
        }
    }
    Ok(())
}

fn configure_connection(connection: &Connection) -> Result<()> {
    connection.busy_timeout(Duration::from_secs(5))?;
    let journal_mode: String =
        connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(StorageError::MigrationInvariant(format!(
            "SQLite refused WAL mode and reported {journal_mode}"
        )));
    }
    connection.execute_batch(
        "PRAGMA synchronous = FULL;
         PRAGMA foreign_keys = ON;
         PRAGMA trusted_schema = OFF;",
    )?;
    let synchronous: i64 = connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    let foreign_keys: i64 =
        connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    if synchronous != 2 || foreign_keys != 1 {
        return Err(StorageError::MigrationInvariant(format!(
            "SQLite durability configuration mismatch: synchronous={synchronous}, foreign_keys={foreign_keys}"
        )));
    }
    Ok(())
}

fn cleanup_fresh_database(database: &Path) {
    for path in [
        database.to_path_buf(),
        PathBuf::from(format!("{}-wal", database.display())),
        PathBuf::from(format!("{}-shm", database.display())),
    ] {
        let _ = fs::remove_file(path);
    }
}

pub(crate) fn validate_identifier(field: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(StorageError::InvalidIdentifier { field })
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn object_file_path(paths: &StoragePaths, hash: &ObjectHash) -> PathBuf {
    object_path(paths.objects(), hash)
}

#[cfg(test)]
pub(crate) fn application_id() -> i32 {
    migration::AER_SQLITE_APPLICATION_ID
}
