use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use aer_repo::{
    FreshnessState, IndexPolicy, RepositoryCapsuleLimits, RepositoryCapsuleScope, RepositoryIndex,
};

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    index_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("aer-ri2-capsule-{}-{nonce}", std::process::id()));
        let repo = root.join("repo");
        let index_path = root.join("index.sqlite");
        fs::create_dir_all(repo.join("src/auth")).expect("src");
        fs::create_dir_all(repo.join("tests")).expect("tests");
        git(&repo, ["init", "-q"]);
        git(&repo, ["config", "user.email", "aer@example.invalid"]);
        git(&repo, ["config", "user.name", "AER Test"]);
        fs::write(
            repo.join("src/auth/token.rs"),
            "pub fn verify_token(token: &str) -> bool { !token.is_empty() }\n",
        )
        .expect("source");
        fs::write(
            repo.join("src/unrelated.rs"),
            "pub fn render_widget() -> bool { true }\n",
        )
        .expect("unrelated");
        fs::write(
            repo.join("tests/auth_test.rs"),
            "#[test]\nfn token_test() { assert!(true); }\n",
        )
        .expect("test");
        git(&repo, ["add", "."]);
        git(&repo, ["commit", "-q", "-m", "capsule fixture"]);
        Self {
            root,
            repo,
            index_path,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn git<const N: usize>(repo: &Path, args: [&str; N]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git");
    assert!(status.success(), "git failed: {args:?}");
}

#[test]
fn file_and_directory_capsules_are_bounded_derived_views_over_current_ri2() {
    let fixture = Fixture::new();
    let mut index =
        RepositoryIndex::open(&fixture.index_path, IndexPolicy::default()).expect("index");
    let report = index.refresh(&fixture.repo).expect("refresh");
    let limits = RepositoryCapsuleLimits {
        max_symbols: 8,
        max_dependencies: 8,
        max_dependents: 8,
        max_tests: 8,
        max_build_targets: 8,
        max_source_anchors: 8,
        max_source_hashes: 8,
    };

    let file = index
        .repository_capsule(
            &report.snapshot.snapshot_id,
            &RepositoryCapsuleScope::File("src/auth/token.rs".to_owned()),
            limits,
        )
        .expect("file capsule");
    assert_eq!(file.canonical_identity, "src/auth/token.rs");
    assert_eq!(file.freshness, FreshnessState::Current);
    assert_eq!(file.producer_version, "ri2-capsule-v1");
    assert!(file.key_symbols.len() <= limits.max_symbols);
    assert!(file.source_anchors.len() <= limits.max_source_anchors);
    assert!(file.source_hashes.len() <= limits.max_source_hashes);
    assert!(
        file.key_symbols
            .iter()
            .any(|symbol| symbol.name == "verify_token")
    );
    assert!(
        file.source_anchors
            .iter()
            .all(|anchor| anchor.path == "src/auth/token.rs")
    );
    assert!(!file.source_hashes.is_empty());

    let directory = index
        .repository_capsule(
            &report.snapshot.snapshot_id,
            &RepositoryCapsuleScope::Directory("src/auth".to_owned()),
            limits,
        )
        .expect("directory capsule");
    assert!(
        directory
            .source_anchors
            .iter()
            .all(|anchor| anchor.path.starts_with("src/auth/"))
    );
    assert!(
        directory
            .key_symbols
            .iter()
            .all(|symbol| symbol.path.starts_with("src/auth/"))
    );
    assert!(
        directory
            .key_symbols
            .iter()
            .all(|symbol| symbol.name != "render_widget")
    );

    let file_again = index
        .repository_capsule(
            &report.snapshot.snapshot_id,
            &RepositoryCapsuleScope::File("src/auth/token.rs".to_owned()),
            limits,
        )
        .expect("repeat capsule");
    assert_eq!(file.capsule_id, file_again.capsule_id);
    assert_eq!(file.source_hashes, file_again.source_hashes);
}
