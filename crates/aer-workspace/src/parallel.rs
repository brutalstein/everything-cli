//! Branch-backed parallel worktree lifecycle and integration primitives.
//!
//! User-owned workspace state is never used as a concurrent worker directory.
//! A captured `WorkspaceSnapshot` is first materialized into an AER-owned
//! integration worktree. If the snapshot contains dirty state, that state is
//! committed to an internal baseline branch so every parallel task can branch
//! from the exact same reproducible tree.

use std::{
    error::Error,
    ffi::OsString,
    fmt,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use aer_exec::{
    CommandSpec, ExecutionPolicy, LocalProcessExecutor, ProcessResult, SideEffectClass,
};

use super::{
    INSPECTION_OUTPUT_LIMIT, OwnedWorktree, WorkspaceError, WorkspaceSnapshot, git_failure,
    parse_nul_paths, run_git, run_git_status,
};

const INTERNAL_AUTHOR_NAME: &str = "Everything AER";
const INTERNAL_AUTHOR_EMAIL: &str = "aer@local.invalid";
const INTERNAL_SNAPSHOT_DATE: &str = "2000-01-01T00:00:00Z";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationMerge {
    pub branch_name: String,
    pub previous_head: String,
    pub resulting_head: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskChangeSet {
    pub branch_name: String,
    pub base_commit: String,
    pub head_commit: String,
    pub changed_paths: Vec<PathBuf>,
    pub dirty: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedWorktreeRecord {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Debug)]
pub struct IntegrationWorktree {
    owned: OwnedWorktree,
    branch_name: String,
    snapshot_commit: String,
}

impl IntegrationWorktree {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.owned.path
    }

    #[must_use]
    pub fn branch_name(&self) -> &str {
        &self.branch_name
    }

    #[must_use]
    pub fn snapshot_commit(&self) -> &str {
        &self.snapshot_commit
    }

    pub fn current_head(&self) -> Result<String, ParallelWorkspaceError> {
        rev_parse_head(&self.owned.path)
    }

    /// Forks a writable task worktree from the current integration head.
    ///
    /// A task branch therefore never shares a writable directory with either
    /// the user or another task. Callers should create all siblings for one
    /// parallel wave before merging any sibling when they need identical bases.
    pub fn fork_task_worktree(
        &self,
        destination: impl AsRef<Path>,
        branch_name: impl Into<String>,
    ) -> Result<TaskWorktree, ParallelWorkspaceError> {
        let branch_name = branch_name.into();
        validate_branch_name(&self.owned.source_repo_root, &branch_name)?;
        let destination = destination.as_ref().to_path_buf();
        ensure_empty_destination(&destination)?;
        let base_commit = self.current_head()?;

        let result = run_git(
            &self.owned.source_repo_root,
            [
                OsString::from("worktree"),
                OsString::from("add"),
                OsString::from("-b"),
                OsString::from(&branch_name),
                destination.as_os_str().to_owned(),
                OsString::from(&base_commit),
            ],
            SideEffectClass::WorkspaceWrite,
            None,
            INSPECTION_OUTPUT_LIMIT,
        );
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&destination);
            return Err(error.into());
        }

        Ok(TaskWorktree {
            owned: OwnedWorktree {
                source_repo_root: self.owned.source_repo_root.clone(),
                path: destination,
                base_commit: base_commit.clone(),
            },
            branch_name,
            integration_base_commit: base_commit,
        })
    }

    /// Merges a locally verified, clean task branch into the AER integration
    /// branch. This operation is *not* acceptance: integration-aware
    /// verification still has to run against `resulting_head`.
    pub fn merge_task(
        &mut self,
        changes: &TaskChangeSet,
    ) -> Result<IntegrationMerge, ParallelWorkspaceError> {
        if changes.dirty {
            return Err(ParallelWorkspaceError::DirtyTaskWorktree(
                changes.branch_name.clone(),
            ));
        }
        if changes.head_commit == changes.base_commit {
            return Err(ParallelWorkspaceError::EmptyTaskChange(
                changes.branch_name.clone(),
            ));
        }
        let previous_head = self.current_head()?;
        if !is_ancestor(
            &self.owned.source_repo_root,
            &changes.base_commit,
            &previous_head,
        )? {
            return Err(ParallelWorkspaceError::StaleTaskBase {
                branch: changes.branch_name.clone(),
                base: changes.base_commit.clone(),
                integration_head: previous_head,
            });
        }

        let merge = run_git(
            &self.owned.path,
            [
                OsString::from("merge"),
                OsString::from("--no-ff"),
                OsString::from("--no-edit"),
                OsString::from("--no-verify"),
                OsString::from(&changes.branch_name),
            ],
            SideEffectClass::WorkspaceWrite,
            None,
            INSPECTION_OUTPUT_LIMIT,
        );
        if let Err(error) = merge {
            let _ = run_git_status(
                &self.owned.path,
                [OsString::from("merge"), OsString::from("--abort")],
                SideEffectClass::WorkspaceWrite,
                None,
                INSPECTION_OUTPUT_LIMIT,
            );
            return Err(ParallelWorkspaceError::Merge(error));
        }
        let resulting_head = self.current_head()?;
        Ok(IntegrationMerge {
            branch_name: changes.branch_name.clone(),
            previous_head,
            resulting_head,
        })
    }

    /// Removes only the worktree registration/directory. The internal branch is
    /// intentionally preserved for evidence/recovery until the caller performs
    /// an explicit branch cleanup after acceptance or abandonment.
    pub fn remove_preserving_branch(self) -> Result<(), ParallelWorkspaceError> {
        self.owned.remove().map_err(Into::into)
    }

    pub fn remove_and_delete_branch(self) -> Result<(), ParallelWorkspaceError> {
        let repo_root = self.owned.source_repo_root.clone();
        let branch = self.branch_name.clone();
        self.owned.remove()?;
        delete_branch(&repo_root, &branch)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct TaskWorktree {
    owned: OwnedWorktree,
    branch_name: String,
    integration_base_commit: String,
}

impl TaskWorktree {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.owned.path
    }

    #[must_use]
    pub fn branch_name(&self) -> &str {
        &self.branch_name
    }

    #[must_use]
    pub fn base_commit(&self) -> &str {
        &self.integration_base_commit
    }

    pub fn change_set(&self) -> Result<TaskChangeSet, ParallelWorkspaceError> {
        let head_commit = rev_parse_head(&self.owned.path)?;
        let status = run_git(
            &self.owned.path,
            [
                OsString::from("status"),
                OsString::from("--porcelain=v1"),
                OsString::from("-z"),
            ],
            SideEffectClass::PureRead,
            None,
            INSPECTION_OUTPUT_LIMIT,
        )?;
        let dirty = !status.stdout.preview.is_empty();
        let changed = run_git(
            &self.owned.path,
            [
                OsString::from("diff"),
                OsString::from("--name-only"),
                OsString::from("-z"),
                OsString::from(format!(
                    "{}..{}",
                    self.integration_base_commit, head_commit
                )),
                OsString::from("--"),
            ],
            SideEffectClass::PureRead,
            None,
            INSPECTION_OUTPUT_LIMIT,
        )?;
        if changed.stdout.truncated {
            return Err(ParallelWorkspaceError::ChangedPathInventoryTooLarge(
                changed.stdout.total_bytes,
            ));
        }
        let mut changed_paths = parse_nul_paths(&changed.stdout.preview)?;
        changed_paths.sort();
        changed_paths.dedup();
        Ok(TaskChangeSet {
            branch_name: self.branch_name.clone(),
            base_commit: self.integration_base_commit.clone(),
            head_commit,
            changed_paths,
            dirty,
        })
    }

    pub fn remove_preserving_branch(self) -> Result<(), ParallelWorkspaceError> {
        self.owned.remove().map_err(Into::into)
    }

    pub fn remove_and_delete_branch(self) -> Result<(), ParallelWorkspaceError> {
        let repo_root = self.owned.source_repo_root.clone();
        let branch = self.branch_name.clone();
        self.owned.remove()?;
        delete_branch(&repo_root, &branch)?;
        Ok(())
    }
}

impl WorkspaceSnapshot {
    /// Creates the AER-owned integration worktree/branch for a snapshot.
    ///
    /// Dirty tracked/untracked user state is first reproduced by the existing
    /// exact materialization path, then captured in one internal baseline commit
    /// with deterministic identity/timestamps. No operation changes the user's
    /// active branch or working directory.
    pub fn materialize_integration_worktree(
        &self,
        destination: impl AsRef<Path>,
        branch_name: impl Into<String>,
    ) -> Result<IntegrationWorktree, ParallelWorkspaceError> {
        let branch_name = branch_name.into();
        validate_branch_name(&self.identity.repo_root, &branch_name)?;
        let owned = self.materialize_owned_worktree(destination)?;
        let switch = run_git(
            &owned.path,
            [
                OsString::from("switch"),
                OsString::from("-c"),
                OsString::from(&branch_name),
            ],
            SideEffectClass::WorkspaceWrite,
            None,
            INSPECTION_OUTPUT_LIMIT,
        );
        if let Err(error) = switch {
            let _ = owned.clone().remove();
            return Err(error.into());
        }

        if worktree_is_dirty(&owned.path)? {
            run_git(
                &owned.path,
                [OsString::from("add"), OsString::from("--all")],
                SideEffectClass::WorkspaceWrite,
                None,
                INSPECTION_OUTPUT_LIMIT,
            )?;
            let commit = run_git_with_env(
                &owned.path,
                [
                    OsString::from("commit"),
                    OsString::from("--no-verify"),
                    OsString::from("--no-gpg-sign"),
                    OsString::from("-m"),
                    OsString::from("AER internal workspace snapshot"),
                ],
                SideEffectClass::WorkspaceWrite,
                &[
                    ("GIT_AUTHOR_NAME", INTERNAL_AUTHOR_NAME),
                    ("GIT_AUTHOR_EMAIL", INTERNAL_AUTHOR_EMAIL),
                    ("GIT_COMMITTER_NAME", INTERNAL_AUTHOR_NAME),
                    ("GIT_COMMITTER_EMAIL", INTERNAL_AUTHOR_EMAIL),
                    ("GIT_AUTHOR_DATE", INTERNAL_SNAPSHOT_DATE),
                    ("GIT_COMMITTER_DATE", INTERNAL_SNAPSHOT_DATE),
                ],
            );
            if let Err(error) = commit {
                let _ = owned.clone().remove();
                return Err(error.into());
            }
        }

        let snapshot_commit = rev_parse_head(&owned.path)?;
        Ok(IntegrationWorktree {
            owned,
            branch_name,
            snapshot_commit,
        })
    }
}

/// Returns branch-backed AER worktrees without deleting anything. Orphan status
/// must be decided by the coordinator against durable lease/task ownership.
pub fn list_managed_worktrees(
    repo_root: impl AsRef<Path>,
    branch_prefix: &str,
) -> Result<Vec<ManagedWorktreeRecord>, ParallelWorkspaceError> {
    if branch_prefix.trim().is_empty() {
        return Err(ParallelWorkspaceError::EmptyBranchPrefix);
    }
    let result = run_git(
        repo_root.as_ref(),
        [
            OsString::from("worktree"),
            OsString::from("list"),
            OsString::from("--porcelain"),
            OsString::from("-z"),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    if result.stdout.truncated {
        return Err(ParallelWorkspaceError::WorktreeInventoryTooLarge(
            result.stdout.total_bytes,
        ));
    }
    let expected_prefix = format!("refs/heads/{branch_prefix}");
    Ok(parse_worktree_porcelain_z(&result.stdout.preview)?
        .into_iter()
        .filter(|record| {
            record
                .branch
                .as_deref()
                .is_some_and(|branch| branch.starts_with(&expected_prefix))
        })
        .collect())
}

/// Prunes only already-missing/stale Git worktree administration records. This
/// must be called only after coordinator-level orphan classification; it never
/// removes a live worktree directory.
pub fn prune_stale_worktree_metadata(
    repo_root: impl AsRef<Path>,
) -> Result<(), ParallelWorkspaceError> {
    run_git(
        repo_root.as_ref(),
        [
            OsString::from("worktree"),
            OsString::from("prune"),
            OsString::from("--expire"),
            OsString::from("now"),
        ],
        SideEffectClass::WorkspaceWrite,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    Ok(())
}

fn ensure_empty_destination(path: &Path) -> Result<(), ParallelWorkspaceError> {
    if path.exists() {
        if fs::read_dir(path)?.next().transpose()?.is_some() {
            return Err(ParallelWorkspaceError::DestinationNotEmpty(
                path.to_path_buf(),
            ));
        }
    } else if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn validate_branch_name(repo_root: &Path, branch: &str) -> Result<(), ParallelWorkspaceError> {
    if branch.trim().is_empty() {
        return Err(ParallelWorkspaceError::InvalidBranchName(branch.to_owned()));
    }
    let result = run_git_status(
        repo_root,
        [
            OsString::from("check-ref-format"),
            OsString::from("--branch"),
            OsString::from(branch),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    if !result.success {
        return Err(ParallelWorkspaceError::InvalidBranchName(branch.to_owned()));
    }
    Ok(())
}

fn rev_parse_head(path: &Path) -> Result<String, ParallelWorkspaceError> {
    let result = run_git(
        path,
        [OsString::from("rev-parse"), OsString::from("HEAD")],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    let head = String::from_utf8_lossy(&result.stdout.preview)
        .trim()
        .to_owned();
    if head.is_empty() {
        return Err(ParallelWorkspaceError::UnexpectedEmptyGitOutput);
    }
    Ok(head)
}

fn worktree_is_dirty(path: &Path) -> Result<bool, ParallelWorkspaceError> {
    let result = run_git(
        path,
        [
            OsString::from("status"),
            OsString::from("--porcelain=v1"),
            OsString::from("-z"),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    Ok(!result.stdout.preview.is_empty())
}

fn is_ancestor(repo_root: &Path, base: &str, head: &str) -> Result<bool, ParallelWorkspaceError> {
    let result = run_git_status(
        repo_root,
        [
            OsString::from("merge-base"),
            OsString::from("--is-ancestor"),
            OsString::from(base),
            OsString::from(head),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    match result.exit_code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(ParallelWorkspaceError::MergeBaseCheck(git_failure(
            "merge-base --is-ancestor",
            &result,
        ))),
    }
}

fn delete_branch(repo_root: &Path, branch: &str) -> Result<(), ParallelWorkspaceError> {
    run_git(
        repo_root,
        [
            OsString::from("branch"),
            OsString::from("-D"),
            OsString::from(branch),
        ],
        SideEffectClass::WorkspaceWrite,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    Ok(())
}

fn run_git_with_env<I>(
    cwd: &Path,
    args: I,
    side_effect: SideEffectClass,
    environment: &[(&str, &str)],
) -> Result<ProcessResult, WorkspaceError>
where
    I: IntoIterator<Item = OsString>,
{
    let policy = ExecutionPolicy::trusted_workspace(
        cwd,
        Duration::from_secs(30),
        INSPECTION_OUTPUT_LIMIT,
    )?;
    let mut spec = CommandSpec::new("git", cwd, side_effect).args(args);
    for (name, value) in environment {
        spec = spec.env(*name, *value);
    }
    let result = LocalProcessExecutor.execute(&policy, spec)?;
    if !result.success {
        return Err(git_failure("git", &result));
    }
    Ok(result)
}

fn parse_worktree_porcelain_z(
    bytes: &[u8],
) -> Result<Vec<ManagedWorktreeRecord>, ParallelWorkspaceError> {
    let mut records = Vec::new();
    let mut current: Option<ManagedWorktreeRecord> = None;
    for field in bytes.split(|byte| *byte == 0) {
        if field.is_empty() {
            if let Some(record) = current.take() {
                records.push(record);
            }
            continue;
        }
        let text = std::str::from_utf8(field)
            .map_err(|_| ParallelWorkspaceError::NonUtf8WorktreeInventory)?;
        if let Some(path) = text.strip_prefix("worktree ") {
            if let Some(record) = current.take() {
                records.push(record);
            }
            current = Some(ManagedWorktreeRecord {
                path: PathBuf::from(path),
                head: None,
                branch: None,
                locked: false,
                prunable: false,
            });
        } else if let Some(record) = current.as_mut() {
            if let Some(head) = text.strip_prefix("HEAD ") {
                record.head = Some(head.to_owned());
            } else if let Some(branch) = text.strip_prefix("branch ") {
                record.branch = Some(branch.to_owned());
            } else if text == "locked" || text.starts_with("locked ") {
                record.locked = true;
            } else if text == "prunable" || text.starts_with("prunable ") {
                record.prunable = true;
            }
        }
    }
    if let Some(record) = current {
        records.push(record);
    }
    Ok(records)
}

#[derive(Debug)]
pub enum ParallelWorkspaceError {
    Workspace(WorkspaceError),
    Merge(WorkspaceError),
    MergeBaseCheck(WorkspaceError),
    Io(std::io::Error),
    InvalidBranchName(String),
    EmptyBranchPrefix,
    DestinationNotEmpty(PathBuf),
    DirtyTaskWorktree(String),
    EmptyTaskChange(String),
    StaleTaskBase {
        branch: String,
        base: String,
        integration_head: String,
    },
    ChangedPathInventoryTooLarge(u64),
    WorktreeInventoryTooLarge(u64),
    NonUtf8WorktreeInventory,
    UnexpectedEmptyGitOutput,
}

impl fmt::Display for ParallelWorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workspace(error) | Self::Merge(error) | Self::MergeBaseCheck(error) => {
                error.fmt(formatter)
            }
            Self::Io(error) => error.fmt(formatter),
            Self::InvalidBranchName(branch) => write!(formatter, "invalid Git branch name: {branch}"),
            Self::EmptyBranchPrefix => formatter.write_str("managed branch prefix must be non-empty"),
            Self::DestinationNotEmpty(path) => write!(
                formatter,
                "parallel worktree destination is not empty: {}",
                path.display()
            ),
            Self::DirtyTaskWorktree(branch) => write!(
                formatter,
                "task worktree must be clean before integration: {branch}"
            ),
            Self::EmptyTaskChange(branch) => {
                write!(formatter, "task branch has no committed change: {branch}")
            }
            Self::StaleTaskBase {
                branch,
                base,
                integration_head,
            } => write!(
                formatter,
                "task branch {branch} base {base} is not an ancestor of integration head {integration_head}"
            ),
            Self::ChangedPathInventoryTooLarge(bytes) => write!(
                formatter,
                "task changed-path inventory exceeds configured capture bound: {bytes} bytes"
            ),
            Self::WorktreeInventoryTooLarge(bytes) => write!(
                formatter,
                "Git worktree inventory exceeds configured capture bound: {bytes} bytes"
            ),
            Self::NonUtf8WorktreeInventory => {
                formatter.write_str("Git worktree inventory contained non-UTF-8 metadata")
            }
            Self::UnexpectedEmptyGitOutput => {
                formatter.write_str("Git returned unexpected empty output")
            }
        }
    }
}

impl Error for ParallelWorkspaceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Workspace(error) | Self::Merge(error) | Self::MergeBaseCheck(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WorkspaceError> for ParallelWorkspaceError {
    fn from(value: WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<std::io::Error> for ParallelWorkspaceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
