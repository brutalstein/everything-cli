use std::{fmt, io, path::PathBuf};

/// Durable-state failures that callers can classify without parsing SQLite or
/// operating-system error strings.
#[derive(Debug)]
pub enum StorageError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Sqlite(rusqlite::Error),
    UnsupportedDatabaseVersion {
        found: u32,
        supported: u32,
    },
    UnrecognizedDatabase(PathBuf),
    UnrecognizedStateDirectory(PathBuf),
    InvalidDatabaseVersion(String),
    MigrationInvariant(String),
    InvalidIdentifier {
        field: &'static str,
    },
    UnsupportedEventSchemaVersion {
        found: u32,
        supported: u32,
    },
    SecretObjectRejected,
    InvalidObjectHash(String),
    ObjectCorrupt {
        expected: String,
        actual: String,
    },
    ObjectMetadataConflict {
        hash: String,
        project_id: String,
    },
    ArtifactNotScoped {
        hash: String,
        project_id: String,
    },
    DanglingCausation(String),
    CausationScopeMismatch {
        event_id: String,
        cause_project: String,
        event_project: String,
    },
    EventIdGeneration(String),
    ProjectionMismatch {
        project_id: String,
    },
    FaultInjected(&'static str),
}

impl StorageError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Sqlite(source) => write!(formatter, "SQLite error: {source}"),
            Self::UnsupportedDatabaseVersion { found, supported } => write!(
                formatter,
                "database schema version {found} is newer than supported version {supported}"
            ),
            Self::UnrecognizedDatabase(path) => write!(
                formatter,
                "existing SQLite database is not a recognized AER state database: {}",
                path.display()
            ),
            Self::UnrecognizedStateDirectory(path) => write!(
                formatter,
                "existing .aer directory contains state not owned by this durable-state layout: {}",
                path.display()
            ),
            Self::InvalidDatabaseVersion(value) => {
                write!(
                    formatter,
                    "invalid durable database schema version: {value}"
                )
            }
            Self::MigrationInvariant(message) => {
                write!(formatter, "migration invariant failed: {message}")
            }
            Self::InvalidIdentifier { field } => {
                write!(formatter, "{field} must be a non-empty identifier")
            }
            Self::UnsupportedEventSchemaVersion { found, supported } => write!(
                formatter,
                "event schema version {found} is unsupported; current version is {supported}"
            ),
            Self::SecretObjectRejected => write!(
                formatter,
                "secret-class data is not permitted in the ordinary content-addressed object store"
            ),
            Self::InvalidObjectHash(hash) => {
                write!(formatter, "invalid SHA-256 object hash: {hash}")
            }
            Self::ObjectCorrupt { expected, actual } => write!(
                formatter,
                "content-addressed object hash mismatch: expected {expected}, found {actual}"
            ),
            Self::ObjectMetadataConflict { hash, project_id } => write!(
                formatter,
                "object {hash} already has conflicting metadata in project {project_id}"
            ),
            Self::ArtifactNotScoped { hash, project_id } => write!(
                formatter,
                "artifact {hash} is not registered for project {project_id}"
            ),
            Self::DanglingCausation(event_id) => {
                write!(formatter, "causation references unknown event {event_id}")
            }
            Self::CausationScopeMismatch {
                event_id,
                cause_project,
                event_project,
            } => write!(
                formatter,
                "causation event {event_id} belongs to project {cause_project}, not {event_project}"
            ),
            Self::EventIdGeneration(message) => {
                write!(
                    formatter,
                    "failed to generate monotonic event id: {message}"
                )
            }
            Self::ProjectionMismatch { project_id } => write!(
                formatter,
                "materialized journal projection does not match deterministic replay for project {project_id}"
            ),
            Self::FaultInjected(point) => write!(formatter, "test fault injected at {point}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Sqlite(source) => Some(source),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(source: rusqlite::Error) -> Self {
        Self::Sqlite(source)
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
