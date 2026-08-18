use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use aer_context::{ContextEngine, ContextPack, ContextPolicy, ContextRequest, ContextTier};
use aer_repo::{IndexPolicy, RepositoryIndex};

static COUNTER: AtomicU64 = AtomicU64::new(0);
const CI_SAFE_DISTRACTOR_FILES: usize = 2_500;

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    index_path: PathBuf,
}

impl Fixture {
    fn large() -> Self {
        let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("aer-context-v2-large-{now}-{nonce}"));
        let repo = root.join("repo");
        let index_path = root.join("index.sqlite");
        fs::create_dir_all(repo.join("src/auth")).expect("target directory");
        fs::create_dir_all(repo.join("generated/unrelated")).expect("unrelated directory");
        git(&repo, ["init", "-q"]);
        git(&repo, ["config", "user.email", "aer@example.invalid"]);
        git(&repo, ["config", "user.name", "AER Test"]);

        fs::write(
            repo.join("src/auth/token.rs"),
            "pub fn verify_token(token: &str) -> bool {\n    !token.is_empty() && !token.contains(\"expired\")\n}\n",
        )
        .expect("target source");
        fs::write(
            repo.join("src/auth/session.rs"),
            "use super::token::verify_token;\npub fn open_session(token: &str) -> bool { verify_token(token) }\n",
        )
        .expect("support source");

        let local_scale = std::env::var("AER_LARGE_REPO_FILES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(CI_SAFE_DISTRACTOR_FILES)
            .max(CI_SAFE_DISTRACTOR_FILES);
        for index in 0..local_scale {
            let shard = index / 250;
            let directory = repo.join(format!("generated/unrelated/shard-{shard:03}"));
            fs::create_dir_all(&directory).expect("generated shard");
            fs::write(
                directory.join(format!("module-{index:05}.txt")),
                format!("unrelated telemetry rendering billing widget module {index}\n"),
            )
            .expect("generated distractor");
        }
        git(&repo, ["add", "."]);
        git(&repo, ["commit", "-q", "-m", "large fixture"]);
        Self {
            root,
            repo,
            index_path,
        }
    }

    fn index(&self) -> RepositoryIndex {
        RepositoryIndex::open(&self.index_path, IndexPolicy::default()).expect("open RI2")
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

#[derive(Debug)]
struct OfflineQuality {
    selected_context_units: u32,
    provider_visible_bytes: usize,
    selected_source_lines: usize,
    redundant_selected_lines: usize,
    overlapping_span_pairs: usize,
    retrieval_stage_count: usize,
    unnecessary_stage_count: usize,
    tier_escalation_count: usize,
    mandatory_like_context_units: u32,
    optional_context_units: u32,
}

fn offline_quality(pack: &ContextPack) -> OfflineQuality {
    let mut covered_lines = BTreeSet::<(String, u32)>::new();
    let mut selected_source_lines = 0_usize;
    let mut redundant_selected_lines = 0_usize;
    let mut overlapping_span_pairs = 0_usize;
    let mut mandatory_like_context_units = 0_u32;
    let mut optional_context_units = 0_u32;

    for item in &pack.items {
        let mandatory_like = item.tier >= ContextTier::SourceSpan
            || !item.required_semantic_ids.is_empty()
            || item.selected_reason.contains("evidence demand:");
        if mandatory_like {
            mandatory_like_context_units =
                mandatory_like_context_units.saturating_add(item.token_cost);
        } else {
            optional_context_units = optional_context_units.saturating_add(item.token_cost);
        }

        for (index, segment) in item.segments.iter().enumerate() {
            for other in item.segments.iter().skip(index + 1) {
                if segment.start_line <= other.end_line && other.start_line <= segment.end_line {
                    overlapping_span_pairs = overlapping_span_pairs.saturating_add(1);
                }
            }
            for line in segment.start_line..=segment.end_line {
                selected_source_lines = selected_source_lines.saturating_add(1);
                if !covered_lines.insert((item.path.clone(), line)) {
                    redundant_selected_lines = redundant_selected_lines.saturating_add(1);
                }
            }
        }
    }

    OfflineQuality {
        selected_context_units: pack.total_token_cost(),
        provider_visible_bytes: pack.items.iter().map(|item| item.rendered_text.len()).sum(),
        selected_source_lines,
        redundant_selected_lines,
        overlapping_span_pairs,
        retrieval_stage_count: pack.retrieval_trace.stages_invoked.len(),
        unnecessary_stage_count: pack.retrieval_trace.unnecessary_stage_count,
        tier_escalation_count: pack.retrieval_trace.tier_escalations,
        mandatory_like_context_units,
        optional_context_units,
    }
}

#[test]
fn enterprise_scale_exact_localization_keeps_model_visible_context_bounded() {
    let fixture = Fixture::large();
    let mut index = fixture.index();
    index
        .refresh(&fixture.repo)
        .expect("index large repository");
    let engine = ContextEngine::new(ContextPolicy::default()).expect("context engine");
    let mut request = ContextRequest::new(
        "large-repo-exact-definition",
        "inspect the exact verify_token definition",
        1,
        6_144,
    );
    request.required_symbols.push("verify_token".to_owned());

    let pack = engine
        .compile(&fixture.repo, &index, &request)
        .expect("localized large-repo pack");
    let metrics = offline_quality(&pack);

    assert_eq!(pack.items.len(), 1, "large repository leaked distractors");
    assert_eq!(pack.items[0].path, "src/auth/token.rs");
    assert_eq!(
        pack.retrieval_trace.stages_invoked,
        vec![aer_context::RetrievalStage::Exact]
    );
    assert_eq!(metrics.retrieval_stage_count, 1);
    assert_eq!(metrics.unnecessary_stage_count, 0);
    assert_eq!(metrics.redundant_selected_lines, 0);
    assert_eq!(metrics.overlapping_span_pairs, 0);
    assert!(metrics.selected_source_lines > 0);
    assert!(metrics.provider_visible_bytes < 16 * 1024);
    assert!(metrics.selected_context_units < request.input_token_budget);
    assert!(metrics.mandatory_like_context_units > 0);
    assert_eq!(metrics.optional_context_units, 0);
    assert!(metrics.tier_escalation_count <= 1);
}

#[test]
fn exact_evidence_payload_is_budget_invariant_and_span_deduplicated() {
    let fixture = Fixture::large();
    let mut index = fixture.index();
    index.refresh(&fixture.repo).expect("index");
    let engine = ContextEngine::new(ContextPolicy::default()).expect("context engine");

    let compile = |budget| {
        let mut request = ContextRequest::new(
            "budget-ceiling-large-repo",
            "inspect the exact verify_token definition",
            1,
            budget,
        );
        request.required_symbols.push("verify_token".to_owned());
        engine
            .compile(&fixture.repo, &index, &request)
            .expect("pack")
    };
    let six_k = compile(6_144);
    let twelve_k = compile(12_288);

    let visible = |pack: &ContextPack| {
        pack.items
            .iter()
            .map(|item| {
                (
                    item.path.clone(),
                    item.tier,
                    item.source_ref.clone(),
                    item.rendered_text.clone(),
                    item.segments.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(visible(&six_k), visible(&twelve_k));
    assert_eq!(
        offline_quality(&six_k).provider_visible_bytes,
        offline_quality(&twelve_k).provider_visible_bytes
    );
    assert_eq!(offline_quality(&six_k).redundant_selected_lines, 0);
    assert_eq!(offline_quality(&six_k).overlapping_span_pairs, 0);
}
