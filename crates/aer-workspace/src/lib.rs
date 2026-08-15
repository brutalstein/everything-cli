//! Git workspace identity, dirty-state snapshots, and owned worktree materialization.

use std::{
    error::Error,
    ffi::OsString,
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use aer_exec::{
    CommandSpec, ExecutionPolicy, LocalProcessExecutor, ProcessResult, SideEffectClass,
};
use sha2::{Digest, Sha256};

const INSPECTION_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteIdentity {
    pub name: String,
    pub urls: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceIdentity {
    pub repo_id: String,
    pub repo_root: PathBuf,
    pub head_commit: String,
    pub branch: Option<String>,
    pub remotes: Vec<RemoteIdentity>,
    pub dirty_tracked_diff_sha256: String,
    pub untracked_inventory_sha256: String,
    pub submodule_state_sha256: String,
    pub tracked_dirty: bool,
    pub untracked_paths: Vec<PathBuf>,
}

impl WorkspaceIdentity {
    pub fn inspect(path: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        Ok(collect_state(path.as_ref(), INSPECTION_OUTPUT_LIMIT)?.identity)
    }

    #[must_use]
    pub fn is_clean(&self) -> bool {
        !self.tracked_dirty && self.untracked_paths.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct SnapshotPolicy {
    pub include_untracked: bool,
    pub max_tracked_patch_bytes: usize,
    pub max_untracked_file_bytes: u64,
    pub max_total_untracked_bytes: u64,
}

impl Default for SnapshotPolicy {
    fn default() -> Self {
        Self {
            include_untracked: true,
            max_tracked_patch_bytes: 32 * 1024 * 1024,
            max_untracked_file_bytes: 16 * 1024 * 1024,
            max_total_untracked_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UntrackedFileSnapshot {
    pub relative_path: PathBuf,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct WorkspaceSnapshot {
    pub identity: WorkspaceIdentity,
    pub tracked_patch: Vec<u8>,
    pub untracked_files: Vec<UntrackedFileSnapshot>,
    pub exact: bool,
}

impl WorkspaceSnapshot {
    pub fn capture(
        path: impl AsRef<Path>,
        policy: &SnapshotPolicy,
    ) -> Result<Self, WorkspaceError> {
        if policy.max_tracked_patch_bytes == 0
            || policy.max_untracked_file_bytes == 0
            || policy.max_total_untracked_bytes == 0
        {
            return Err(WorkspaceError::InvalidSnapshotPolicy);
        }
        let before = collect_state(path.as_ref(), policy.max_tracked_patch_bytes)?;
        let mut untracked_files = Vec::new();
        let mut total_untracked = 0_u64;

        if policy.include_untracked {
            for relative_path in &before.untracked_paths {
                validate_relative_path(relative_path)?;
                let source = before.identity.repo_root.join(relative_path);
                let metadata = fs::symlink_metadata(&source).map_err(WorkspaceError::Io)?;
                if !metadata.file_type().is_file() {
                    return Err(WorkspaceError::UnsupportedUntrackedEntry(
                        relative_path.clone(),
                    ));
                }
                if metadata.len() > policy.max_untracked_file_bytes {
                    return Err(WorkspaceError::UntrackedFileTooLarge {
                        path: relative_path.clone(),
                        bytes: metadata.len(),
                    });
                }
                total_untracked = total_untracked
                    .checked_add(metadata.len())
                    .ok_or(WorkspaceError::SnapshotSizeOverflow)?;
                if total_untracked > policy.max_total_untracked_bytes {
                    return Err(WorkspaceError::UntrackedTotalTooLarge(total_untracked));
                }
                let bytes = fs::read(&source).map_err(WorkspaceError::Io)?;
                untracked_files.push(UntrackedFileSnapshot {
                    relative_path: relative_path.clone(),
                    sha256: sha256(&bytes),
                    bytes,
                });
            }
        }

        let after = collect_state(path.as_ref(), policy.max_tracked_patch_bytes)?;
        if before.identity.head_commit != after.identity.head_commit
            || before.identity.dirty_tracked_diff_sha256 != after.identity.dirty_tracked_diff_sha256
            || before.identity.untracked_inventory_sha256
                != after.identity.untracked_inventory_sha256
        {
            return Err(WorkspaceError::WorkspaceChangedDuringSnapshot);
        }
        for file in &untracked_files {
            let current = fs::read(after.identity.repo_root.join(&file.relative_path))
                .map_err(WorkspaceError::Io)?;
            if sha256(&current) != file.sha256 {
                return Err(WorkspaceError::WorkspaceChangedDuringSnapshot);
            }
        }

        let exact = policy.include_untracked || after.untracked_paths.is_empty();
        Ok(Self {
            identity: after.identity,
            tracked_patch: after.tracked_patch,
            untracked_files,
            exact,
        })
    }

    pub fn materialize_owned_worktree(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<OwnedWorktree, WorkspaceError> {
        if !self.exact {
            return Err(WorkspaceError::InexactSnapshotCannotMaterialize);
        }
        let destination = destination.as_ref().to_path_buf();
        if destination.exists() {
            let mut entries = fs::read_dir(&destination).map_err(WorkspaceError::Io)?;
            if entries
                .next()
                .transpose()
                .map_err(WorkspaceError::Io)?
                .is_some()
            {
                return Err(WorkspaceError::DestinationNotEmpty(destination));
            }
        } else if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(WorkspaceError::Io)?;
        }

        let result = self.materialize_inner(&destination);
        if result.is_err() {
            let _ = remove_worktree_registration(&self.identity.repo_root, &destination);
            let _ = fs::remove_dir_all(&destination);
        }
        result
    }

    fn materialize_inner(&self, destination: &Path) -> Result<OwnedWorktree, WorkspaceError> {
        run_git(
            &self.identity.repo_root,
            [
                OsString::from("worktree"),
                OsString::from("add"),
                OsString::from("--detach"),
                destination.as_os_str().to_owned(),
                OsString::from(&self.identity.head_commit),
            ],
            SideEffectClass::WorkspaceWrite,
            None,
            INSPECTION_OUTPUT_LIMIT,
        )?;

        if !self.tracked_patch.is_empty() {
            run_git(
                destination,
                [
                    OsString::from("apply"),
                    OsString::from("--binary"),
                    OsString::from("--whitespace=nowarn"),
                    OsString::from("-"),
                ],
                SideEffectClass::WorkspaceWrite,
                Some(self.tracked_patch.clone()),
                INSPECTION_OUTPUT_LIMIT,
            )?;
        }

        for file in &self.untracked_files {
            validate_relative_path(&file.relative_path)?;
            let target = destination.join(&file.relative_path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(WorkspaceError::Io)?;
            }
            let mut handle = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(WorkspaceError::Io)?;
            handle.write_all(&file.bytes).map_err(WorkspaceError::Io)?;
            handle.sync_all().map_err(WorkspaceError::Io)?;
        }

        let materialized = WorkspaceIdentity::inspect(destination)?;
        if materialized.head_commit != self.identity.head_commit
            || materialized.dirty_tracked_diff_sha256 != self.identity.dirty_tracked_diff_sha256
            || materialized.untracked_inventory_sha256 != self.identity.untracked_inventory_sha256
        {
            return Err(WorkspaceError::MaterializedSnapshotMismatch);
        }

        Ok(OwnedWorktree {
            source_repo_root: self.identity.repo_root.clone(),
            path: destination.to_path_buf(),
            base_commit: self.identity.head_commit.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedWorktree {
    source_repo_root: PathBuf,
    pub path: PathBuf,
    pub base_commit: String,
}

impl OwnedWorktree {
    pub fn remove(self) -> Result<(), WorkspaceError> {
        remove_worktree_registration(&self.source_repo_root, &self.path)
    }
}

#[derive(Debug)]
struct CollectedState {
    identity: WorkspaceIdentity,
    tracked_patch: Vec<u8>,
    untracked_paths: Vec<PathBuf>,
}

fn collect_state(path: &Path, patch_limit: usize) -> Result<CollectedState, WorkspaceError> {
    let root_result = run_git(
        path,
        [
            OsString::from("rev-parse"),
            OsString::from("--show-toplevel"),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    let repo_root = PathBuf::from(single_line(&root_result.stdout.preview)?)
        .canonicalize()
        .map_err(WorkspaceError::Io)?;

    let head = run_git(
        &repo_root,
        [OsString::from("rev-parse"), OsString::from("HEAD")],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    let head_commit = single_line(&head.stdout.preview)?;

    let branch = run_git_status(
        &repo_root,
        [
            OsString::from("symbolic-ref"),
            OsString::from("--quiet"),
            OsString::from("--short"),
            OsString::from("HEAD"),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    let branch = if branch.success {
        Some(single_line(&branch.stdout.preview)?)
    } else if branch.exit_code == Some(1) {
        None
    } else {
        return Err(git_failure("symbolic-ref", &branch));
    };

    let tracked = run_git(
        &repo_root,
        [
            OsString::from("diff"),
            OsString::from("--binary"),
            OsString::from("--no-ext-diff"),
            OsString::from("HEAD"),
            OsString::from("--"),
        ],
        SideEffectClass::PureRead,
        None,
        patch_limit,
    )?;
    if tracked.stdout.truncated {
        return Err(WorkspaceError::TrackedPatchTooLarge(
            tracked.stdout.total_bytes,
        ));
    }
    let tracked_patch = tracked.stdout.preview.clone();

    let untracked = run_git(
        &repo_root,
        [
            OsString::from("ls-files"),
            OsString::from("--others"),
            OsString::from("--exclude-standard"),
            OsString::from("-z"),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    if untracked.stdout.truncated {
        return Err(WorkspaceError::UntrackedInventoryTooLarge(
            untracked.stdout.total_bytes,
        ));
    }
    let untracked_paths = parse_nul_paths(&untracked.stdout.preview)?;

    let submodules = run_git(
        &repo_root,
        [
            OsString::from("submodule"),
            OsString::from("status"),
            OsString::from("--recursive"),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    if submodules.stdout.truncated {
        return Err(WorkspaceError::SubmoduleInventoryTooLarge(
            submodules.stdout.total_bytes,
        ));
    }

    let remotes = inspect_remotes(&repo_root)?;
    let repo_id = compute_repo_id(&repo_root, &remotes);
    let identity = WorkspaceIdentity {
        repo_id,
        repo_root,
        head_commit,
        branch,
        remotes,
        dirty_tracked_diff_sha256: sha256(&tracked_patch),
        untracked_inventory_sha256: sha256(&untracked.stdout.preview),
        submodule_state_sha256: sha256(&submodules.stdout.preview),
        tracked_dirty: !tracked_patch.is_empty(),
        untracked_paths: untracked_paths.clone(),
    };
    Ok(CollectedState {
        identity,
        tracked_patch,
        untracked_paths,
    })
}

fn inspect_remotes(repo_root: &Path) -> Result<Vec<RemoteIdentity>, WorkspaceError> {
    let names = run_git(
        repo_root,
        [OsString::from("remote")],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    let mut remotes = Vec::new();
    for name in String::from_utf8_lossy(&names.stdout.preview)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let urls = run_git(
            repo_root,
            [
                OsString::from("remote"),
                OsString::from("get-url"),
                OsString::from("--all"),
                OsString::from(name),
            ],
            SideEffectClass::PureRead,
            None,
            INSPECTION_OUTPUT_LIMIT,
        )?;
        let mut sanitized = String::from_utf8_lossy(&urls.stdout.preview)
            .lines()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(sanitize_remote_url)
            .collect::<Vec<_>>();
        sanitized.sort();
        sanitized.dedup();
        remotes.push(RemoteIdentity {
            name: name.to_owned(),
            urls: sanitized,
        });
    }
    remotes.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(remotes)
}

fn compute_repo_id(repo_root: &Path, remotes: &[RemoteIdentity]) -> String {
    let mut hasher = Sha256::new();
    if remotes.iter().any(|remote| !remote.urls.is_empty()) {
        hasher.update(b"git-remotes\0");
        for remote in remotes {
            hasher.update(remote.name.as_bytes());
            hasher.update([0]);
            for url in &remote.urls {
                hasher.update(url.as_bytes());
                hasher.update([0xff]);
            }
        }
    } else {
        hasher.update(b"local-root\0");
        hasher.update(repo_root.to_string_lossy().as_bytes());
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn sanitize_remote_url(raw: &str) -> String {
    let without_fragment = raw.split('#').next().unwrap_or(raw);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    for scheme in ["https://", "http://", "ssh://"] {
        if let Some(rest) = without_query.strip_prefix(scheme) {
            if let Some(at) = rest.rfind('@') {
                return format!("{scheme}{}", &rest[at + 1..]);
            }
        }
    }
    without_query.to_owned()
}

fn remove_worktree_registration(repo_root: &Path, path: &Path) -> Result<(), WorkspaceError> {
    run_git(
        repo_root,
        [
            OsString::from("worktree"),
            OsString::from("remove"),
            OsString::from("--force"),
            path.as_os_str().to_owned(),
        ],
        SideEffectClass::WorkspaceWrite,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    Ok(())
}

fn run_git<I>(
    cwd: &Path,
    args: I,
    side_effect: SideEffectClass,
    stdin: Option<Vec<u8>>,
    capture_limit: usize,
) -> Result<ProcessResult, WorkspaceError>
where
    I: IntoIterator<Item = OsString>,
{
    let result = run_git_status(cwd, args, side_effect, stdin, capture_limit)?;
    if !result.success {
        return Err(git_failure("git", &result));
    }
    Ok(result)
}

fn run_git_status<I>(
    cwd: &Path,
    args: I,
    side_effect: SideEffectClass,
    stdin: Option<Vec<u8>>,
    capture_limit: usize,
) -> Result<ProcessResult, WorkspaceError>
where
    I: IntoIterator<Item = OsString>,
{
    let policy = ExecutionPolicy::trusted_workspace(cwd, Duration::from_secs(30), capture_limit)?;
    let mut spec = CommandSpec::new("git", cwd, side_effect).args(args);
    if let Some(stdin) = stdin {
        spec = spec.stdin(stdin);
    }
    LocalProcessExecutor
        .execute(&policy, spec)
        .map_err(WorkspaceError::Execution)
}

fn git_failure(operation: &str, result: &ProcessResult) -> WorkspaceError {
    WorkspaceError::GitFailed {
        operation: operation.to_owned(),
        exit_code: result.exit_code,
        stderr: String::from_utf8_lossy(&result.stderr.preview)
            .trim()
            .to_owned(),
    }
}

fn single_line(bytes: &[u8]) -> Result<String, WorkspaceError> {
    let value = String::from_utf8_lossy(bytes).trim().to_owned();
    if value.is_empty() {
        return Err(WorkspaceError::UnexpectedEmptyGitOutput);
    }
    Ok(value)
}

fn validate_relative_path(path: &Path) -> Result<(), WorkspaceError> {
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(WorkspaceError::UnsafeRelativePath(path.to_path_buf()));
    }
    Ok(())
}

#[cfg(unix)]
fn parse_nul_paths(bytes: &[u8]) -> Result<Vec<PathBuf>, WorkspaceError> {
    use std::os::unix::ffi::OsStringExt;

    Ok(bytes
        .split(|byte| *byte == 0)
        .filter(|segment| !segment.is_empty())
        .map(|segment| PathBuf::from(OsString::from_vec(segment.to_vec())))
        .collect())
}

#[cfg(windows)]
fn parse_nul_paths(bytes: &[u8]) -> Result<Vec<PathBuf>, WorkspaceError> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            String::from_utf8(segment.to_vec())
                .map(PathBuf::from)
                .map_err(|_| WorkspaceError::NonUtf8GitPath)
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug)]
pub enum WorkspaceError {
    InvalidSnapshotPolicy,
    TrackedPatchTooLarge(u64),
    UntrackedInventoryTooLarge(u64),
    SubmoduleInventoryTooLarge(u64),
    UntrackedFileTooLarge {
        path: PathBuf,
        bytes: u64,
    },
    UntrackedTotalTooLarge(u64),
    SnapshotSizeOverflow,
    UnsupportedUntrackedEntry(PathBuf),
    WorkspaceChangedDuringSnapshot,
    InexactSnapshotCannotMaterialize,
    DestinationNotEmpty(PathBuf),
    MaterializedSnapshotMismatch,
    UnsafeRelativePath(PathBuf),
    NonUtf8GitPath,
    UnexpectedEmptyGitOutput,
    GitFailed {
        operation: String,
        exit_code: Option<i32>,
        stderr: String,
    },
    Execution(aer_exec::ExecutionError),
    Io(std::io::Error),
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSnapshotPolicy => {
                formatter.write_str("snapshot policy limits must be nonzero")
            }
            Self::TrackedPatchTooLarge(bytes) => {
                write!(
                    formatter,
                    "tracked dirty patch exceeds configured bound: {bytes} bytes"
                )
            }
            Self::UntrackedInventoryTooLarge(bytes) => {
                write!(
                    formatter,
                    "untracked inventory exceeds configured bound: {bytes} bytes"
                )
            }
            Self::SubmoduleInventoryTooLarge(bytes) => {
                write!(
                    formatter,
                    "submodule inventory exceeds configured bound: {bytes} bytes"
                )
            }
            Self::UntrackedFileTooLarge { path, bytes } => write!(
                formatter,
                "untracked file exceeds snapshot bound ({} bytes): {}",
                bytes,
                path.display()
            ),
            Self::UntrackedTotalTooLarge(bytes) => {
                write!(
                    formatter,
                    "untracked snapshot exceeds total bound: {bytes} bytes"
                )
            }
            Self::SnapshotSizeOverflow => formatter.write_str("snapshot byte accounting overflow"),
            Self::UnsupportedUntrackedEntry(path) => write!(
                formatter,
                "untracked snapshot entry is not a regular file: {}",
                path.display()
            ),
            Self::WorkspaceChangedDuringSnapshot => formatter.write_str(
                "workspace changed while the immutable snapshot was being captured; retry required",
            ),
            Self::InexactSnapshotCannotMaterialize => formatter.write_str(
                "snapshot excludes user-owned untracked state and cannot be materialized as exact",
            ),
            Self::DestinationNotEmpty(path) => {
                write!(
                    formatter,
                    "owned worktree destination is not empty: {}",
                    path.display()
                )
            }
            Self::MaterializedSnapshotMismatch => formatter.write_str(
                "materialized worktree does not reproduce the captured tracked/untracked identity",
            ),
            Self::UnsafeRelativePath(path) => {
                write!(
                    formatter,
                    "unsafe repository-relative path: {}",
                    path.display()
                )
            }
            Self::NonUtf8GitPath => formatter.write_str("Git emitted a non-UTF-8 path on Windows"),
            Self::UnexpectedEmptyGitOutput => {
                formatter.write_str("Git returned unexpected empty output")
            }
            Self::GitFailed {
                operation,
                exit_code,
                stderr,
            } => write!(
                formatter,
                "Git operation failed ({operation}, exit {exit_code:?}): {stderr}"
            ),
            Self::Execution(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Execution(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<aer_exec::ExecutionError> for WorkspaceError {
    fn from(value: aer_exec::ExecutionError) -> Self {
        Self::Execution(value)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command, time::SystemTime};

    use super::{SnapshotPolicy, WorkspaceIdentity, WorkspaceSnapshot};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "everything-workspace-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    fn git(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("git command");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn initialized_repo(label: &str) -> std::path::PathBuf {
        let repo = temp_dir(label);
        git(&repo, &["init"]);
        git(&repo, &["config", "user.name", "Everything Tests"]);
        git(&repo, &["config", "user.email", "tests@everything.invalid"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        fs::write(repo.join("tracked.txt"), "base\n").expect("tracked file");
        git(&repo, &["add", "tracked.txt"]);
        git(&repo, &["commit", "-m", "baseline"]);
        repo
    }

    #[test]
    fn dirty_snapshot_materializes_without_mutating_user_worktree() {
        let repo = initialized_repo("dirty");
        let original_branch = git(&repo, &["branch", "--show-current"]);
        fs::write(repo.join("tracked.txt"), "dirty\n").expect("dirty tracked");
        fs::write(repo.join("untracked.txt"), "local-only\n").expect("untracked");

        let snapshot = WorkspaceSnapshot::capture(&repo, &SnapshotPolicy::default())
            .expect("capture dirty snapshot");
        assert!(snapshot.exact);
        assert!(snapshot.identity.tracked_dirty);
        assert_eq!(snapshot.identity.untracked_paths.len(), 1);

        let owned_path = temp_dir("owned-parent").join("owned worktree");
        let owned = snapshot
            .materialize_owned_worktree(&owned_path)
            .expect("materialize worktree");
        assert_eq!(
            fs::read_to_string(owned.path.join("tracked.txt")).expect("owned tracked"),
            "dirty\n"
        );
        assert_eq!(
            fs::read_to_string(owned.path.join("untracked.txt")).expect("owned untracked"),
            "local-only\n"
        );
        assert_eq!(
            fs::read_to_string(repo.join("tracked.txt")).expect("user tracked"),
            "dirty\n"
        );
        assert_eq!(git(&repo, &["branch", "--show-current"]), original_branch);

        owned.remove().expect("remove worktree");
        fs::remove_dir_all(repo).expect("cleanup repo");
        let parent = owned_path.parent().expect("owned parent");
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn excluding_untracked_state_marks_snapshot_inexact_and_refuses_materialization() {
        let repo = initialized_repo("inexact");
        fs::write(repo.join("untracked.txt"), "local-only\n").expect("untracked");
        let policy = SnapshotPolicy {
            include_untracked: false,
            ..SnapshotPolicy::default()
        };
        let snapshot = WorkspaceSnapshot::capture(&repo, &policy).expect("snapshot");
        assert!(!snapshot.exact);
        let destination = temp_dir("inexact-destination").join("owned");
        assert!(snapshot.materialize_owned_worktree(&destination).is_err());
        fs::remove_dir_all(repo).expect("cleanup repo");
        let _ = fs::remove_dir_all(destination.parent().expect("parent"));
    }

    #[test]
    fn remote_credentials_are_removed_from_workspace_identity() {
        let repo = initialized_repo("remote-redaction");
        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://user:super-secret@example.com/org/repo.git?token=also-secret",
            ],
        );
        let identity = WorkspaceIdentity::inspect(&repo).expect("identity");
        let url = &identity.remotes[0].urls[0];
        assert_eq!(url, "https://example.com/org/repo.git");
        assert!(!url.contains("super-secret"));
        assert!(!url.contains("also-secret"));
        fs::remove_dir_all(repo).expect("cleanup repo");
    }
}
