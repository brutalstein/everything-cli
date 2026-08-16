use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use aer_context::{
    ContextEngine, ContextError, ContextPolicy, ContextRequest, RuntimeHint, estimate_tokens,
    evaluate_context_pack,
};
use aer_repo::{IndexPolicy, RepoError, RepositoryIndex, SemanticAnchor};

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
        let base = std::env::temp_dir().join(format!("aer-context-{now}-{nonce}"));
        let repo = base.join("repo");
        let index = base.join("index.sqlite");
        fs::create_dir_all(repo.join("src")).expect("src");
        fs::create_dir_all(repo.join("tests")).expect("tests");
        fs::create_dir_all(repo.join("docs")).expect("docs");
        git(&repo, ["init"]);
        git(&repo, ["config", "user.email", "aer@example.invalid"]);
        git(&repo, ["config", "user.name", "AER Test"]);

        fs::write(
            repo.join("src/auth.rs"),
            "pub fn verify_token(token: &str) -> bool {\n    !token.is_empty() && !token.contains(\"expired\")\n}\n",
        )
        .expect("auth source");
        fs::write(
            repo.join("src/session.rs"),
            "use crate::auth::verify_token;\npub fn open_session(token: &str) -> bool { verify_token(token) }\n",
        )
        .expect("session source");
        fs::write(
            repo.join("tests/auth_test.rs"),
            "#[test]\nfn expired_token_is_rejected() {\n    assert!(!verify_token(\"expired\"));\n}\n",
        )
        .expect("auth test");
        for index in 0..12 {
            let content = format!(
                "# unrelated subsystem {index}\n{}\n",
                "rendering cache widget layout background telemetry ".repeat(120)
            );
            fs::write(
                repo.join("docs").join(format!("unrelated-{index}.md")),
                content,
            )
            .expect("irrelevant source");
        }
        git(&repo, ["add", "."]);
        git(&repo, ["commit", "-m", "initial"]);
        Self { base, repo, index }
    }

    fn open_index(&self) -> RepositoryIndex {
        RepositoryIndex::open(&self.index, IndexPolicy::default()).expect("repository index")
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

fn indexed_fixture() -> (Fixture, RepositoryIndex, String) {
    let fixture = Fixture::new();
    let mut index = fixture.open_index();
    let report = index.refresh(&fixture.repo).expect("refresh index");
    index
        .replace_semantic_anchors(
            &report.snapshot.snapshot_id,
            &[SemanticAnchor {
                kind: "requirement".to_owned(),
                id: "req-auth-expiry".to_owned(),
                text: "expired authentication tokens must be rejected and verified by tests"
                    .to_owned(),
            }],
        )
        .expect("semantic links");
    (fixture, index, report.snapshot.snapshot_id)
}

#[test]
fn context_bench_beats_naive_whole_context_yield_with_full_provenance() {
    let (fixture, index, snapshot_id) = indexed_fixture();
    let engine = ContextEngine::new(ContextPolicy::default()).expect("context engine");
    let mut request = ContextRequest::new(
        "task-auth-expiry",
        "fix expired authentication token verification and its tests",
        1,
        1200,
    );
    request
        .required_semantic_ids
        .push("req-auth-expiry".to_owned());
    request.runtime_hints.push(RuntimeHint {
        path: "src/auth.rs".to_owned(),
        score_milli: 950,
        reason: "failing authentication verification points here".to_owned(),
    });

    let pack = engine
        .compile(&fixture.repo, &index, &request)
        .expect("compile Context Pack");
    assert_eq!(pack.repo_snapshot, snapshot_id);
    assert!(pack.total_token_cost() <= request.input_token_budget);
    assert_eq!(
        pack.items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        pack.items.len(),
        "one redundancy group per repository path"
    );
    assert!(pack.items.iter().any(|item| item.path == "src/auth.rs"));
    assert!(
        pack.items
            .iter()
            .any(|item| item.path == "tests/auth_test.rs")
    );
    assert!(pack.items.iter().all(|item| {
        item.content_hash.starts_with("sha256:")
            && item
                .segments
                .iter()
                .all(|segment| segment.sha256.starts_with("sha256:"))
    }));
    engine
        .verify_fidelity(&fixture.repo, &index, &pack)
        .expect("fidelity verification");

    let naive_tokens = ["src/auth.rs", "src/session.rs", "tests/auth_test.rs"]
        .into_iter()
        .map(|path| fs::read_to_string(fixture.repo.join(path)).expect("source"))
        .chain((0..12).map(|idx| {
            fs::read_to_string(
                fixture
                    .repo
                    .join("docs")
                    .join(format!("unrelated-{idx}.md")),
            )
            .expect("doc")
        }))
        .map(|content| u64::from(estimate_tokens(&content)))
        .sum::<u64>();
    let metrics = evaluate_context_pack(
        &pack,
        &["src/auth.rs".to_owned(), "tests/auth_test.rs".to_owned()],
        naive_tokens,
    );
    assert_eq!(metrics.recall_milli, 1000);
    assert!(metrics.selected_tokens < metrics.naive_tokens);
    assert!(metrics.selected_yield_micros > metrics.naive_yield_micros);
}

#[test]
fn stale_workspace_is_rejected_before_a_pack_can_be_reused() {
    let (fixture, index, _) = indexed_fixture();
    let engine = ContextEngine::new(ContextPolicy::default()).expect("context engine");
    fs::write(
        fixture.repo.join("src/auth.rs"),
        "pub fn verify_token(_: &str) -> bool { true }\n",
    )
    .expect("mutate source");
    let request = ContextRequest::new("task", "expired authentication token", 1, 800);
    let error = engine
        .compile(&fixture.repo, &index, &request)
        .expect_err("stale index must fail closed");
    assert!(matches!(
        error,
        ContextError::Repository(RepoError::StaleIndex { .. })
    ));
}

#[test]
fn mandatory_semantic_coverage_cannot_disappear_silently() {
    let (fixture, index, _) = indexed_fixture();
    let engine = ContextEngine::new(ContextPolicy::default()).expect("context engine");
    let mut request = ContextRequest::new("task", "authentication", 1, 800);
    request
        .required_semantic_ids
        .push("req-does-not-exist".to_owned());
    let error = engine
        .compile(&fixture.repo, &index, &request)
        .expect_err("unknown mandatory semantic coverage must fail");
    assert!(matches!(
        error,
        ContextError::MandatoryCoverageUnavailable(ref id) if id == "req-does-not-exist"
    ));
}

#[test]
fn token_budget_is_a_hard_admission_limit() {
    let (fixture, index, _) = indexed_fixture();
    let engine = ContextEngine::new(ContextPolicy::default()).expect("context engine");
    let mut request = ContextRequest::new("task", "expired authentication token", 1, 33);
    request
        .required_semantic_ids
        .push("req-auth-expiry".to_owned());
    let error = engine
        .compile(&fixture.repo, &index, &request)
        .expect_err("mandatory source cannot overflow token budget");
    assert!(matches!(error, ContextError::BudgetTooSmall { .. }));
}
