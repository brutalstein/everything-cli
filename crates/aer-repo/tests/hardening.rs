use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use aer_repo::{
    IndexPolicy, RepoError, RepositoryIndex, RuntimeObservation, SearchQuery,
};

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
        let base = std::env::temp_dir().join(format!("aer-repo-hardening-{now}-{nonce}"));
        let repo = base.join("repo");
        let index = base.join("index.sqlite");
        fs::create_dir_all(repo.join("src")).expect("fixture dirs");
        git(&repo, ["init"]);
        git(&repo, ["config", "user.email", "aer@example.invalid"]);
        git(&repo, ["config", "user.name", "AER Test"]);
        fs::write(
            repo.join("src/lib.rs"),
            "pub fn verify_token(token: &str) -> bool { !token.is_empty() }\n",
        )
        .expect("fixture source");
        git(&repo, ["add", "."]);
        git(&repo, ["commit", "-m", "initial"]);
        Self { base, repo, index }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn git<const N: usize>(repo: &Path, args: [&str; N]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git command");
    assert!(status.success());
}

#[test]
fn query_budget_is_fail_closed() {
    let fixture = Fixture::new();
    let policy = IndexPolicy {
        max_query_bytes: 8,
        ..IndexPolicy::default()
    };
    let mut index = RepositoryIndex::open(&fixture.index, policy).expect("index");
    let report = index.refresh(&fixture.repo).expect("refresh");
    let error = index
        .search(
            &report.snapshot.snapshot_id,
            &SearchQuery::new("this query is intentionally too large"),
        )
        .expect_err("oversized query must fail");
    assert!(matches!(error, RepoError::QueryTooLarge(_)));
}

#[test]
fn runtime_observations_only_link_to_files_in_the_exact_snapshot() {
    let fixture = Fixture::new();
    let mut index = RepositoryIndex::open(&fixture.index, IndexPolicy::default()).expect("index");
    let report = index.refresh(&fixture.repo).expect("refresh");
    let observations = vec![
        RuntimeObservation {
            observation_id: "obs-known".to_owned(),
            path: "src/lib.rs".to_owned(),
            line: Some(1),
            summary: "verification failed here".to_owned(),
        },
        RuntimeObservation {
            observation_id: "obs-unknown".to_owned(),
            path: "src/missing.rs".to_owned(),
            line: None,
            summary: "must not create a fabricated repository edge".to_owned(),
        },
    ];
    let links = index
        .replace_runtime_observations(&report.snapshot.snapshot_id, &observations)
        .expect("runtime links");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].observation_id, "obs-known");
    assert_eq!(links[0].path, "src/lib.rs");
    assert!(links[0].content_sha256.is_some());
}

#[test]
fn snapshot_retention_never_deletes_current_snapshot_pointer() {
    let fixture = Fixture::new();
    let policy = IndexPolicy {
        retained_snapshots: 2,
        ..IndexPolicy::default()
    };
    let mut index = RepositoryIndex::open(&fixture.index, policy).expect("index");
    let first = index.refresh(&fixture.repo).expect("first refresh");

    fs::write(
        fixture.repo.join("src/lib.rs"),
        "pub fn verify_token(token: &str) -> bool { token.len() > 1 }\n",
    )
    .expect("second source");
    let second = index.refresh(&fixture.repo).expect("second refresh");

    fs::write(
        fixture.repo.join("src/lib.rs"),
        "pub fn verify_token(token: &str) -> bool { token.len() > 2 }\n",
    )
    .expect("third source");
    let third = index.refresh(&fixture.repo).expect("third refresh");

    assert_ne!(first.snapshot.snapshot_id, second.snapshot.snapshot_id);
    assert_ne!(second.snapshot.snapshot_id, third.snapshot.snapshot_id);
    assert_eq!(
        index
            .current_snapshot_id(&third.snapshot.repo_id)
            .expect("current snapshot"),
        Some(third.snapshot.snapshot_id.clone())
    );
    let result = index
        .search_current(&fixture.repo, &SearchQuery::new("verify token"))
        .expect("current search");
    assert_eq!(result.snapshot_id, third.snapshot.snapshot_id);
}
