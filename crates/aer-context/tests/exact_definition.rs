//! Exact-identifier / exact-definition retrieval regression.
//!
//! A task that names an identifier and asks for its concrete value must receive
//! the defining source span, or the compilation must fail closed. A nearby
//! structural span that contains the enclosing item but omits the requested
//! assignment is not acceptable evidence.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use aer_context::{ContextEngine, ContextError, ContextPolicy, ContextRequest};
use aer_repo::{IndexPolicy, RepositoryIndex};

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
        let base = std::env::temp_dir().join(format!("aer-context-exact-{now}-{nonce}"));
        let repo = base.join("repo");
        let index = base.join("index.sqlite");
        fs::create_dir_all(repo.join("src")).expect("src");
        git(&repo, ["init"]);
        git(&repo, ["config", "user.email", "aer@example.invalid"]);
        git(&repo, ["config", "user.name", "AER Test"]);
        fs::write(repo.join("src/model_context.rs"), model_context_source())
            .expect("model context source");
        fs::write(
            repo.join("src/other.rs"),
            "pub fn compile_report() -> u32 {\n    // capsule compile version reporting helper\n    0\n}\n",
        )
        .expect("decoy source");
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

/// Mirrors the shape that defeated span selection in production: a long file
/// whose lexical anchor sits far above the assignment the task asks about, and a
/// second `compile` definition in the same file with a different value.
fn model_context_source() -> String {
    let mut source = String::from(
        "//! architecture capsule compile version capsule compile version\n\
         pub const ARCHITECTURE_POLICY_VERSION: &str = \"architecture-context-v3\";\n\n\
         pub struct ArchitectureContextCapsule {\n    pub version: u32,\n}\n\n\
         pub struct ModelContextEnvelope {\n    pub version: u32,\n}\n\n",
    );
    // Push the definition far below the lexical anchor so no fixed-size window
    // anchored at the top of the file can reach the assignment by luck.
    for index in 0..200 {
        source.push_str(&format!("pub const PREAMBLE_{index}: u32 = {index};\n"));
    }
    source.push('\n');
    source.push_str("impl ArchitectureContextCapsule {\n    pub fn compile() -> Self {\n");
    for index in 0..60 {
        source.push_str(&format!(
            "        let _capsule_filler_{index} = ARCHITECTURE_POLICY_VERSION.len();\n"
        ));
    }
    source.push_str("        Self { version: 3 }\n    }\n}\n\n");
    source.push_str("impl ModelContextEnvelope {\n    pub fn compile() -> Self {\n");
    for index in 0..60 {
        source.push_str(&format!(
            "        let _envelope_filler_{index} = ARCHITECTURE_POLICY_VERSION.len();\n"
        ));
    }
    source.push_str("        Self { version: 7 }\n    }\n}\n");
    source
}

fn indexed_fixture() -> (Fixture, RepositoryIndex) {
    let fixture = Fixture::new();
    let mut index = fixture.open_index();
    index.refresh(&fixture.repo).expect("refresh index");
    (fixture, index)
}

fn provider_policy() -> ContextPolicy {
    ContextPolicy {
        version: "provider-context-economy-v1".to_owned(),
        max_candidates: 64,
        max_items: 10,
        max_span_lines: 48,
        max_tier3_lines: 96,
        omitted_high_rank_limit: 8,
        ..ContextPolicy::default()
    }
}

fn request(budget: u32) -> ContextRequest {
    ContextRequest::new(
        "task-capsule-version",
        "what integer version does `ArchitectureContextCapsule::compile` assign to the compiled capsule?",
        1,
        budget,
    )
}

#[test]
fn named_definition_is_retrieved_with_its_defining_assignment() {
    let (fixture, index) = indexed_fixture();
    let engine = ContextEngine::new(provider_policy()).expect("context engine");
    let mut demand = request(6 * 1024);
    demand
        .required_symbols
        .push("ArchitectureContextCapsule::compile".to_owned());

    let pack = engine
        .compile(&fixture.repo, &index, &demand)
        .expect("compile Context Pack");

    let rendered = pack
        .items
        .iter()
        .map(|item| item.rendered_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("Self { version: 3 }"),
        "the exact defining assignment must be present: {rendered}"
    );
    assert!(
        !rendered.contains("Self { version: 7 }"),
        "the unrelated definition must not be pulled in: {rendered}"
    );
    assert!(pack.total_token_cost() <= demand.input_token_budget);
    engine
        .verify_fidelity(&fixture.repo, &index, &pack)
        .expect("fidelity verification");
}

#[test]
fn a_named_definition_that_cannot_be_covered_fails_closed() {
    let (fixture, index) = indexed_fixture();
    let engine = ContextEngine::new(provider_policy()).expect("context engine");

    let mut unknown = request(6 * 1024);
    unknown
        .required_symbols
        .push("ArchitectureContextCapsule::not_a_symbol".to_owned());
    assert!(matches!(
        engine.compile(&fixture.repo, &index, &unknown),
        Err(ContextError::ExactDefinitionUnavailable(_))
    ));

    let mut ambiguous = request(6 * 1024);
    ambiguous.required_symbols.push("compile".to_owned());
    let policy = ContextPolicy {
        max_definitions_per_symbol: 1,
        ..provider_policy()
    };
    let strict = ContextEngine::new(policy).expect("context engine");
    assert!(matches!(
        strict.compile(&fixture.repo, &index, &ambiguous),
        Err(ContextError::ExactDefinitionAmbiguous { .. })
    ));

    let oversized = ContextEngine::new(ContextPolicy {
        max_required_definition_lines: 8,
        ..provider_policy()
    })
    .expect("context engine");
    let mut demand = request(6 * 1024);
    demand
        .required_symbols
        .push("ArchitectureContextCapsule::compile".to_owned());
    assert!(matches!(
        oversized.compile(&fixture.repo, &index, &demand),
        Err(ContextError::ExactDefinitionTooLarge { .. })
    ));

    let mut tight = request(256);
    tight
        .required_symbols
        .push("ArchitectureContextCapsule::compile".to_owned());
    assert!(
        matches!(
            engine.compile(&fixture.repo, &index, &tight),
            Err(ContextError::BudgetTooSmall { .. })
        ),
        "a budget that cannot hold the definition must fail closed, never truncate it"
    );
}

#[test]
fn exact_coverage_survives_context_economy_selection_pressure() {
    let (fixture, index) = indexed_fixture();
    // Enough budget for exactly one item beyond the mandatory definition.
    let engine = ContextEngine::new(ContextPolicy {
        max_items: 2,
        ..provider_policy()
    })
    .expect("context engine");
    let mut demand = request(6 * 1024);
    demand
        .required_symbols
        .push("ArchitectureContextCapsule::compile".to_owned());

    let pack = engine
        .compile(&fixture.repo, &index, &demand)
        .expect("compile Context Pack");
    let defining = pack
        .items
        .iter()
        .find(|item| item.path == "src/model_context.rs")
        .expect("defining file is selected");
    assert!(defining.rendered_text.contains("Self { version: 3 }"));
    assert!(
        defining
            .selected_reason
            .contains("exact definition: ArchitectureContextCapsule::compile")
    );
    assert!(pack.total_token_cost() <= demand.input_token_budget);
}

#[test]
#[ignore = "documents the pre-fix retrieval gap; selection heuristics may legitimately change"]
fn baseline_without_required_symbols_can_miss_the_defining_assignment() {
    let (fixture, index) = indexed_fixture();
    let engine = ContextEngine::new(provider_policy()).expect("context engine");
    let pack = engine
        .compile(&fixture.repo, &index, &request(6 * 1024))
        .expect("compile Context Pack");
    let rendered = pack
        .items
        .iter()
        .map(|item| item.rendered_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !rendered.contains("Self { version: 3 }"),
        "unqualified retrieval happened to include the defining span: {rendered}"
    );
}
