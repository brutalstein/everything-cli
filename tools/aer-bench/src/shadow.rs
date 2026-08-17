//! Filtered, Git-backed shadow copies of the repository under measurement.
//!
//! A benchmark that can retrieve its own answer key measures nothing. Every
//! harness therefore compiles context against a shadow of the repository with
//! harness material removed, and the shadow is a real Git worktree with fixed
//! identity so ordinary snapshot-bound Repository Intelligence semantics still
//! apply to it.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{HarnessError, preview};

const MAX_SHADOW_FILES: usize = 50_000;
const MAX_SHADOW_BYTES: u64 = 256 * 1024 * 1024;

/// A temporary directory removed when the value is dropped.
pub struct TempRoot {
    pub path: PathBuf,
}

impl TempRoot {
    /// Creates a uniquely named directory under the system temp directory.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::Clock`] when the system clock predates the epoch,
    /// or an IO error when the directory cannot be created.
    pub fn new(prefix: &str) -> Result<Self, HarnessError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| HarnessError::Clock)?
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Repository-relative paths a harness must keep out of the shadow.
pub type ExclusionFilter<'a> = &'a dyn Fn(&Path) -> bool;

/// A filtered copy of a repository, tracked by its own Git repository.
pub struct ShadowWorkspace {
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Default)]
struct CopyStats {
    files: usize,
    bytes: u64,
}

impl ShadowWorkspace {
    /// Copies `source` into a fresh shadow, skipping paths the filter rejects.
    ///
    /// When `source` is a Git worktree only tracked files are copied. A
    /// filesystem walk would also sweep in ignored local tool output — indexer
    /// caches, generated graphs, scratch reports — which is neither repository
    /// truth nor reproducible across machines, and which can then be selected as
    /// task evidence.
    ///
    /// # Errors
    ///
    /// Returns an error when the copy escapes the root, exceeds the size limits,
    /// or the shadow repository cannot be initialised.
    pub fn copy_from(source: &Path, excluded: ExclusionFilter<'_>) -> Result<Self, HarnessError> {
        let root = TempRoot::new("everything-bench-shadow")?;
        let path = root.path.clone();
        let mut stats = CopyStats::default();
        match tracked_files(source)? {
            Some(tracked) => copy_tracked(source, &tracked, &path, excluded, &mut stats)?,
            None => copy_tree(source, source, &path, excluded, &mut stats)?,
        }
        initialize_shadow_repository(&path)?;
        // Ownership moves to the returned value, whose own Drop removes it.
        std::mem::forget(root);
        Ok(Self {
            path,
            files: stats.files,
            bytes: stats.bytes,
        })
    }
}

impl Drop for ShadowWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Paths every harness must exclude regardless of its own additions.
#[must_use]
pub fn is_always_excluded(relative: &Path) -> bool {
    let normalized = normalize(relative);
    normalized == ".git"
        || normalized.starts_with(".git/")
        || normalized == "target"
        || normalized.starts_with("target/")
        || normalized == ".aer"
        || normalized.starts_with(".aer/")
}

/// Repository-relative path with forward slashes, for stable comparisons.
#[must_use]
pub fn normalize(relative: &Path) -> String {
    relative.to_string_lossy().replace('\\', "/")
}

fn tracked_files(source: &Path) -> Result<Option<Vec<PathBuf>>, HarnessError> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--cached", "--exclude-standard"])
        .current_dir(source)
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    Ok(Some(
        listing
            .split('\0')
            .filter(|entry| !entry.is_empty())
            .map(PathBuf::from)
            .collect(),
    ))
}

fn copy_tracked(
    source: &Path,
    tracked: &[PathBuf],
    destination: &Path,
    excluded: ExclusionFilter<'_>,
    stats: &mut CopyStats,
) -> Result<(), HarnessError> {
    fs::create_dir_all(destination)?;
    for relative in tracked {
        if is_always_excluded(relative) || excluded(relative) {
            continue;
        }
        let source_path = source.join(relative);
        let metadata = match fs::symlink_metadata(&source_path) {
            Ok(metadata) => metadata,
            // A tracked path can be absent from the worktree (deleted but not
            // yet staged). Skipping keeps the shadow a subset of real content.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() {
            continue;
        }
        let destination_path = destination.join(relative);
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)?;
        }
        account(stats, metadata.len())?;
        fs::copy(&source_path, destination_path)?;
    }
    Ok(())
}

fn copy_tree(
    root: &Path,
    source: &Path,
    destination: &Path,
    excluded: ExclusionFilter<'_>,
    stats: &mut CopyStats,
) -> Result<(), HarnessError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let relative = source_path
            .strip_prefix(root)
            .map_err(|_| HarnessError::ShadowEscape(source_path.clone()))?;
        if is_always_excluded(relative) || excluded(relative) {
            continue;
        }
        let file_type = entry.file_type()?;
        let destination_path = destination.join(entry.file_name());
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            copy_tree(root, &source_path, &destination_path, excluded, stats)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        account(stats, entry.metadata()?.len())?;
        fs::copy(&source_path, destination_path)?;
    }
    Ok(())
}

fn account(stats: &mut CopyStats, bytes: u64) -> Result<(), HarnessError> {
    stats.files = stats.files.saturating_add(1);
    stats.bytes = stats.bytes.saturating_add(bytes);
    if stats.files > MAX_SHADOW_FILES || stats.bytes > MAX_SHADOW_BYTES {
        return Err(HarnessError::ShadowLimit {
            files: stats.files,
            bytes: stats.bytes,
        });
    }
    Ok(())
}

/// Turns a directory into a deterministic single-commit Git repository.
///
/// Snapshot-bound retrieval needs a commit to resolve, and two shadows of the
/// same content must produce the same commit id, so identity and dates are
/// fixed rather than inherited from the operator's Git configuration.
pub fn initialize_shadow_repository(path: &Path) -> Result<(), HarnessError> {
    run_shadow_git(path, &["init", "--quiet"])?;
    run_shadow_git(path, &["symbolic-ref", "HEAD", "refs/heads/aer-shadow"])?;
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("core.filemode", "false"),
        ("commit.gpgSign", "false"),
        ("core.hooksPath", ".git/aer-no-hooks"),
        ("user.name", "AER Shadow"),
        ("user.email", "shadow@aer.invalid"),
    ] {
        run_shadow_git(path, &["config", key, value])?;
    }
    run_shadow_git(
        path,
        &[
            "remote",
            "add",
            "aer-shadow",
            "aer-shadow://filtered-workspace",
        ],
    )?;
    commit_shadow_snapshot(path, "AER filtered shadow snapshot")
}

/// Stages everything and commits it under the fixed shadow identity.
///
/// Use this rather than a plain `git commit` whenever a shadow gains content
/// after initialization; inheriting the operator's identity or the wall clock
/// would make the resulting commit id machine-specific.
pub fn commit_shadow_snapshot(path: &Path, message: &str) -> Result<(), HarnessError> {
    run_shadow_git(path, &["add", "--all", "--", "."])?;
    let output = Command::new("git")
        .args(["commit", "--quiet", "--no-gpg-sign", "--message", message])
        .current_dir(path)
        .env("GIT_AUTHOR_NAME", "AER Shadow")
        .env("GIT_AUTHOR_EMAIL", "shadow@aer.invalid")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "AER Shadow")
        .env("GIT_COMMITTER_EMAIL", "shadow@aer.invalid")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .output()?;
    if !output.status.success() {
        return Err(shadow_git_failure("commit", &output));
    }
    Ok(())
}

/// Runs a Git command inside a shadow and returns its trimmed stdout.
pub fn run_shadow_git(path: &Path, args: &[&str]) -> Result<String, HarnessError> {
    let output = Command::new("git").args(args).current_dir(path).output()?;
    if !output.status.success() {
        return Err(shadow_git_failure(
            &format!("git {}", args.join(" ")),
            &output,
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn shadow_git_failure(label: &str, output: &std::process::Output) -> HarnessError {
    HarnessError::ShadowGit {
        command: label.to_owned(),
        exit_code: output.status.code(),
        detail: preview(&String::from_utf8_lossy(&output.stderr)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_output_and_repository_metadata_are_always_excluded() {
        for path in [
            ".git",
            ".git/config",
            "target",
            "target/debug/x",
            ".aer",
            ".aer/state.db",
        ] {
            assert!(
                is_always_excluded(Path::new(path)),
                "{path} must never enter a shadow"
            );
        }
        for path in ["crates/aer-core/src/root.rs", "docs/00_READ_ME_FIRST.md"] {
            assert!(
                !is_always_excluded(Path::new(path)),
                "{path} is repository truth"
            );
        }
    }

    #[test]
    fn normalization_is_separator_independent() {
        assert_eq!(
            normalize(Path::new("crates\\aer-core\\src")),
            "crates/aer-core/src"
        );
        assert_eq!(
            normalize(Path::new("crates/aer-core/src")),
            "crates/aer-core/src"
        );
    }

    #[test]
    fn a_prefix_that_merely_shares_a_name_is_not_excluded() {
        assert!(!is_always_excluded(Path::new("targeting/notes.md")));
        assert!(!is_always_excluded(Path::new(".github/workflows/ci.yml")));
    }
}
