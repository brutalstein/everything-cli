//! Cross-process exclusive mutation ownership for one logical repository.
//!
//! The lock file is placed in a caller-owned runtime directory, never inside the
//! user's Git working tree. File existence is not authority: the OS lock held by
//! the open handle is. A stale file after a crash is therefore harmless.

use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions, TryLockError},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use aer_exec::lowercase_hex;
use sha2::{Digest, Sha256};

use crate::WorkspaceIdentity;

#[derive(Debug)]
pub struct WorkspaceMutationLock {
    file: File,
    path: PathBuf,
}

impl WorkspaceMutationLock {
    /// Attempts to become the single mutating coordinator for this repository.
    ///
    /// This is intentionally non-blocking. Contention is visible to the caller
    /// instead of silently hanging an interactive CLI or scheduler.
    pub fn try_acquire(
        workspace: &WorkspaceIdentity,
        runtime_root: impl AsRef<Path>,
    ) -> Result<Self, WorkspaceLockError> {
        let runtime_root = runtime_root.as_ref();
        fs::create_dir_all(runtime_root).map_err(WorkspaceLockError::Io)?;

        let key = lowercase_hex(Sha256::digest(workspace.repo_id.as_bytes()).as_ref());
        let path = runtime_root.join(format!("workspace-{key}.lock"));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            // Never truncate before winning the OS lock: doing so could mutate
            // the current owner's diagnostic metadata while it still owns the
            // repository. Metadata is replaced only after try_lock succeeds.
            .truncate(false)
            .open(&path)
            .map_err(WorkspaceLockError::Io)?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(WorkspaceLockError::AlreadyLocked {
                    repo_id: workspace.repo_id.clone(),
                    path,
                });
            }
            Err(TryLockError::Error(error)) => return Err(WorkspaceLockError::Io(error)),
        }

        write_owner_metadata(&mut file, workspace)?;
        Ok(Self { file, path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Explicitly releases the lock. Dropping the value also closes the handle
    /// and therefore releases the OS lock.
    pub fn release(self) -> Result<(), WorkspaceLockError> {
        self.file.unlock().map_err(WorkspaceLockError::Io)
    }
}

fn write_owner_metadata(
    file: &mut File,
    workspace: &WorkspaceIdentity,
) -> Result<(), WorkspaceLockError> {
    file.set_len(0).map_err(WorkspaceLockError::Io)?;
    file.seek(SeekFrom::Start(0))
        .map_err(WorkspaceLockError::Io)?;
    writeln!(file, "product=everything").map_err(WorkspaceLockError::Io)?;
    writeln!(file, "repo_id={}", workspace.repo_id).map_err(WorkspaceLockError::Io)?;
    writeln!(file, "pid={}", std::process::id()).map_err(WorkspaceLockError::Io)?;
    file.sync_data().map_err(WorkspaceLockError::Io)
}

#[derive(Debug)]
pub enum WorkspaceLockError {
    AlreadyLocked { repo_id: String, path: PathBuf },
    Io(std::io::Error),
}

impl fmt::Display for WorkspaceLockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyLocked { repo_id, path } => write!(
                formatter,
                "workspace mutation ownership is already held for {repo_id}: {}",
                path.display()
            ),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl Error for WorkspaceLockError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::AlreadyLocked { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::SystemTime};

    use super::{WorkspaceLockError, WorkspaceMutationLock};
    use crate::WorkspaceIdentity;

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "everything-workspace-lock-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    fn identity(repo_id: &str) -> WorkspaceIdentity {
        WorkspaceIdentity {
            repo_id: repo_id.to_owned(),
            repo_root: PathBuf::from("repo"),
            head_commit: "0123456789abcdef".to_owned(),
            branch: Some("main".to_owned()),
            remotes: Vec::new(),
            dirty_tracked_diff_sha256: "tracked".to_owned(),
            untracked_inventory_sha256: "untracked".to_owned(),
            submodule_state_sha256: "submodule".to_owned(),
            tracked_dirty: false,
            untracked_paths: Vec::new(),
        }
    }

    #[test]
    fn second_mutating_coordinator_fails_fast_for_same_repository() {
        let root = temp_dir("exclusive");
        let workspace = identity("sha256:same-repository");
        let first = WorkspaceMutationLock::try_acquire(&workspace, &root).expect("first lock");
        assert!(matches!(
            WorkspaceMutationLock::try_acquire(&workspace, &root),
            Err(WorkspaceLockError::AlreadyLocked { .. })
        ));
        drop(first);
        WorkspaceMutationLock::try_acquire(&workspace, &root).expect("reacquire after drop");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn different_repository_identities_do_not_contend() {
        let root = temp_dir("independent");
        let first = WorkspaceMutationLock::try_acquire(&identity("sha256:first"), &root)
            .expect("first repo");
        let second = WorkspaceMutationLock::try_acquire(&identity("sha256:second"), &root)
            .expect("second repo");
        assert_ne!(first.path(), second.path());
        assert!(!first.path().to_string_lossy().contains(':'));
        drop((first, second));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn explicit_release_allows_immediate_reacquisition() {
        let root = temp_dir("release");
        let workspace = identity("sha256:release");
        WorkspaceMutationLock::try_acquire(&workspace, &root)
            .expect("lock")
            .release()
            .expect("release");
        WorkspaceMutationLock::try_acquire(&workspace, &root).expect("reacquire");
        fs::remove_dir_all(root).expect("cleanup");
    }
}
