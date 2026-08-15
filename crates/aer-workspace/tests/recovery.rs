use std::{fs, path::Path, process::Command, time::SystemTime};

use aer_workspace::{SnapshotPolicy, WorkspaceSnapshot};

fn temp_dir(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "everything-workspace-recovery-{label}-{}-{nonce}",
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

fn initialized_repo() -> std::path::PathBuf {
    let repo = temp_dir("repo");
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
fn failed_materialization_removes_owned_worktree_registration_and_directory() {
    let repo = initialized_repo();
    let mut snapshot = WorkspaceSnapshot::capture(&repo, &SnapshotPolicy::default())
        .expect("capture clean snapshot");

    // Force failure only after `git worktree add` succeeds. This exercises the
    // cleanup path rather than an early preflight rejection.
    snapshot.tracked_patch = b"this is intentionally not a git patch\n".to_vec();

    let parent = temp_dir("owned-parent");
    let destination = parent.join("owned worktree with spaces");
    assert!(snapshot.materialize_owned_worktree(&destination).is_err());
    assert!(!destination.exists());

    let registrations = git(&repo, &["worktree", "list", "--porcelain"]);
    assert!(!registrations.contains(&destination.to_string_lossy().to_string()));

    fs::remove_dir_all(repo).expect("cleanup repo");
    fs::remove_dir_all(parent).expect("cleanup parent");
}
