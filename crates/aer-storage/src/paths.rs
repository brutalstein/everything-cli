use std::path::{Path, PathBuf};

/// Filesystem locations owned by one local AER durable state directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePaths {
    workspace_root: PathBuf,
    state_root: PathBuf,
    database: PathBuf,
    objects: PathBuf,
    tmp: PathBuf,
    backups: PathBuf,
}

impl StoragePaths {
    #[must_use]
    pub fn for_workspace(workspace_root: impl AsRef<Path>) -> Self {
        let workspace_root = workspace_root.as_ref().to_path_buf();
        let state_root = workspace_root.join(".aer");
        Self {
            database: state_root.join("state.db"),
            objects: state_root.join("objects"),
            tmp: state_root.join("tmp"),
            backups: state_root.join("backups"),
            workspace_root,
            state_root,
        }
    }

    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    #[must_use]
    pub fn database(&self) -> &Path {
        &self.database
    }

    #[must_use]
    pub fn objects(&self) -> &Path {
        &self.objects
    }

    #[must_use]
    pub fn tmp(&self) -> &Path {
        &self.tmp
    }

    #[must_use]
    pub fn backups(&self) -> &Path {
        &self.backups
    }
}

/// Observable SQLite durability settings used by diagnostics and conformance tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurabilityDiagnostics {
    pub journal_mode: String,
    pub synchronous: i64,
    pub foreign_keys: bool,
}
