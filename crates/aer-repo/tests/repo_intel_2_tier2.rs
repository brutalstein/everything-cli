use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use aer_repo::{CapabilityTier, FreshnessState, IndexPolicy, RepositoryIndex};

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    base: PathBuf,
    repo: PathBuf,
    index: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let base = std::env::temp_dir().join(format!("aer-ri2-tier2-{now}-{nonce}"));
        let repo = base.join("repo");
        let index = base.join("index.sqlite");
        fs::create_dir_all(repo.join("src")).expect("root src");
        fs::create_dir_all(repo.join("helper/src")).expect("helper src");
        fs::write(
            repo.join("Cargo.toml"),
            r#"[workspace]
members = ["helper"]
resolver = "3"

[package]
name = "ri2-tier2-fixture"
version = "0.1.0"
edition = "2024"

[dependencies]
helper = { path = "helper" }
"#,
        )
        .expect("root manifest");
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn root_marker() -> bool { helper::helper_marker() }\n",
        )
        .expect("root source");
        fs::write(
            repo.join("helper/Cargo.toml"),
            r#"[package]
name = "helper"
version = "0.1.0"
edition = "2024"
"#,
        )
        .expect("helper manifest");
        fs::write(
            repo.join("helper/src/lib.rs"),
            "pub fn helper_marker() -> bool { true }\n",
        )
        .expect("helper source");
        run(&repo, "cargo", &["generate-lockfile", "--offline"]);
        run(&repo, "git", &["init"]);
        run(
            &repo,
            "git",
            &["config", "user.email", "aer@example.invalid"],
        );
        run(&repo, "git", &["config", "user.name", "AER Test"]);
        run(&repo, "git", &["add", "."]);
        run(&repo, "git", &["commit", "-m", "initial"]);
        Self { base, repo, index }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn run(repo: &Path, program: &str, args: &[&str]) {
    let status = Command::new(program)
        .args(args)
        .current_dir(repo)
        .status()
        .expect("fixture command");
    assert!(status.success(), "{program} {args:?} failed");
}

#[test]
fn tier2_project_view_resolves_local_dependency_and_is_environment_bound() {
    let fixture = Fixture::new();
    let mut index =
        RepositoryIndex::open(&fixture.index, IndexPolicy::default()).expect("open index");
    let first = index.refresh(&fixture.repo).expect("first refresh");
    let snapshot = first.snapshot.snapshot_id.clone();

    let views = index.ri2_view_states(&snapshot).expect("view states");
    let project = views
        .iter()
        .find(|view| view.view_name == "project")
        .expect("project view");
    assert_eq!(project.freshness, FreshnessState::Current);
    assert_eq!(project.capability_tier, CapabilityTier::Tier2Project);
    assert!(project.environment_fingerprint.is_some());

    let packages = index.build_packages(&snapshot).expect("packages");
    assert_eq!(packages.len(), 2);
    assert!(packages.iter().any(|package| package.name == "helper"));
    assert!(
        packages
            .iter()
            .any(|package| package.name == "ri2-tier2-fixture")
    );
    let dependencies = index.project_dependencies(&snapshot).expect("dependencies");
    assert!(dependencies.iter().any(|dependency| {
        dependency.target_name == "helper" && dependency.target_package_id.is_some()
    }));

    assert!(
        index
            .refresh(&fixture.repo)
            .expect("stable refresh")
            .already_current
    );
    drop(index);

    let connection = rusqlite::Connection::open(&fixture.index).expect("direct index open");
    connection
        .execute(
            "UPDATE ri2_view_state SET environment_fingerprint='stale-env-v0' WHERE snapshot_id=? AND view_name='project'",
            [&snapshot],
        )
        .expect("simulate environment provenance drift");
    drop(connection);

    let mut index =
        RepositoryIndex::open(&fixture.index, IndexPolicy::default()).expect("reopen index");
    let rebuilt = index.refresh(&fixture.repo).expect("environment rebuild");
    assert_eq!(rebuilt.snapshot.snapshot_id, snapshot);
    assert!(!rebuilt.already_current);
    let project = index
        .ri2_view_states(&snapshot)
        .expect("rebuilt views")
        .into_iter()
        .find(|view| view.view_name == "project")
        .expect("rebuilt project view");
    assert!(project.environment_fingerprint.is_some());
    assert_ne!(
        project.environment_fingerprint.as_deref(),
        Some("stale-env-v0")
    );
}
