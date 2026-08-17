from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one source block, found {count}")
    return text.replace(old, new, 1)


def replace_between(text: str, start: str, end: str, replacement: str, label: str) -> str:
    start_index = text.find(start)
    if start_index < 0:
        raise SystemExit(f"{label}: start marker not found")
    end_index = text.find(end, start_index + len(start))
    if end_index < 0:
        raise SystemExit(f"{label}: end marker not found")
    return text[:start_index] + replacement + text[end_index:]


# ---------------------------------------------------------------------------
# RI2: freshness without paying for lexical retrieval.
# ---------------------------------------------------------------------------
path = "crates/aer-repo/src/lib.rs"
text = read(path)
old = '''    pub fn search_current(
        &self,
        workspace_root: impl AsRef<Path>,
        query: &SearchQuery,
    ) -> Result<SearchResult, RepoError> {
        let current = snapshot_identity(&WorkspaceSnapshot::capture(
            workspace_root.as_ref(),
            &SnapshotPolicy::default(),
        )?)?;
        let indexed = self
            .current_snapshot_id(&current.repo_id)?
            .ok_or_else(|| RepoError::UnknownSnapshot(current.snapshot_id.clone()))?;
        if indexed != current.snapshot_id {
            return Err(RepoError::StaleIndex {
                indexed,
                current: current.snapshot_id,
            });
        }
        self.search(&indexed, query)
    }
'''
new = '''    pub fn search_current(
        &self,
        workspace_root: impl AsRef<Path>,
        query: &SearchQuery,
    ) -> Result<SearchResult, RepoError> {
        let indexed = self.verified_current_snapshot_id(workspace_root)?;
        self.search(&indexed, query)
    }

    /// Returns the exact indexed snapshot for the current workspace without
    /// forcing a lexical query merely to establish snapshot freshness.
    /// Context Economy uses this so deterministic exact evidence can terminate
    /// discovery before a broader retrieval family is invoked.
    pub fn verified_current_snapshot_id(
        &self,
        workspace_root: impl AsRef<Path>,
    ) -> Result<String, RepoError> {
        let current = snapshot_identity(&WorkspaceSnapshot::capture(
            workspace_root.as_ref(),
            &SnapshotPolicy::default(),
        )?)?;
        let indexed = self
            .current_snapshot_id(&current.repo_id)?
            .ok_or_else(|| RepoError::UnknownSnapshot(current.snapshot_id.clone()))?;
        if indexed != current.snapshot_id {
            return Err(RepoError::StaleIndex {
                indexed,
                current: current.snapshot_id,
            });
        }
        self.ensure_snapshot(&indexed)?;
        Ok(indexed)
    }
'''
text = replace_once(text, old, new, "repo verified snapshot")
write(path, text)


# ---------------------------------------------------------------------------
# Context model: typed evidence demands + retrieval trace.
# ---------------------------------------------------------------------------
path = "crates/aer-context/src/model.rs"
text = read(path)
marker = '''#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHint {
'''
insert = '''/// A typed information requirement compiled before Context Economy selects
/// model-visible evidence. `minimum_coverage` is a count of independent
/// repository candidates; the token budget is only an upper bound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceDemand {
    pub demand_id: String,
    pub kind: EvidenceDemandKind,
    pub target: EvidenceDemandTarget,
    pub minimum_tier: ContextTier,
    pub required_provenance: EvidenceProvenance,
    pub minimum_coverage: u16,
    pub expansion_policy: EvidenceExpansionPolicy,
    pub importance_milli: u16,
    pub verification_critical: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceDemandKind {
    ExactDefinition,
    RequirementContext,
    RuntimeEvidence,
    EditTarget,
    SupportingContext,
    ChangeImpact,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvidenceDemandTarget {
    Symbol(String),
    SemanticId(String),
    Path(String),
    Objective,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceProvenance {
    ExactSource,
    IndexedSource,
    DerivedGraph,
    RuntimeObserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceExpansionPolicy {
    Never,
    ExactDefinition,
    BoundedSourceSpan,
    BoundedNeighborhood,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetrievalStage {
    Exact,
    Lexical,
    Structural,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RetrievalTrace {
    pub stages_invoked: Vec<RetrievalStage>,
    pub demands_total: usize,
    pub demands_satisfied: usize,
    pub tier_escalations: usize,
    pub unnecessary_stage_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHint {
'''
text = replace_once(text, marker, insert, "context demand model")
text = replace_once(
    text,
    '''    pub omitted_high_rank_items: Vec<String>,
    pub source_hashes: Vec<String>,
''',
    '''    pub omitted_high_rank_items: Vec<String>,
    pub source_hashes: Vec<String>,
    pub evidence_demands: Vec<EvidenceDemand>,
    pub retrieval_trace: RetrievalTrace,
''',
    "context pack audit fields",
)
text = replace_once(
    text,
    '''    BudgetTooSmall {
''',
    '''    EvidenceDemandUnsatisfied(String),
    BudgetTooSmall {
''',
    "context demand error variant",
)
text = replace_once(
    text,
    '''            Self::BudgetTooSmall {
''',
    '''            Self::EvidenceDemandUnsatisfied(demand_id) => write!(
                f,
                "required context evidence demand could not be satisfied: {demand_id}"
            ),
            Self::BudgetTooSmall {
''',
    "context demand error display",
)
write(path, text)


# ---------------------------------------------------------------------------
# Context engine: progressive retrieval + evidence-sufficiency selection.
# ---------------------------------------------------------------------------
path = "crates/aer-context/src/lib.rs"
text = read(path)
start = '''        let workspace_root = workspace_root.as_ref();
'''
end = '''        for semantic_id in &request.required_semantic_ids {
'''
replacement = '''        let workspace_root = workspace_root.as_ref();
        let snapshot_id = index.verified_current_snapshot_id(workspace_root)?;
        let demands = compile_evidence_demands(request);
        let mut retrieval_trace = RetrievalTrace {
            demands_total: demands.len(),
            ..RetrievalTrace::default()
        };
        let mut candidates = BTreeMap::<String, Candidate>::new();

        if demands.iter().any(demand_needs_lexical) {
            retrieval_trace.stages_invoked.push(RetrievalStage::Lexical);
            let lexical = index.search(
                &snapshot_id,
                &SearchQuery {
                    text: request.objective.clone(),
                    limit: self.policy.max_candidates.min(64),
                    min_score_micros: 100_000,
                },
            )?;
            for (rank, hit) in lexical.hits.into_iter().enumerate() {
                let candidate = candidates
                    .entry(hit.path.clone())
                    .or_insert_with(|| Candidate::from_hit(&hit));
                candidate.merge_hit(&hit);
                candidate.lexical_rank = Some(rank + 1);
                candidate
                    .reasons
                    .insert("lexical/symbol retrieval".to_owned());
            }
        }

        if !request.required_semantic_ids.is_empty()
            || !request.required_symbols.is_empty()
            || !request.runtime_hints.is_empty()
        {
            retrieval_trace.stages_invoked.push(RetrievalStage::Exact);
        }

'''
text = replace_between(text, start, end, replacement, "compile progressive entry")

start = '''        let seed_paths = ranked_seed_paths(&candidates, self.policy.max_impact_seeds);
'''
end = '''        for candidate in candidates.values_mut() {
'''
replacement = '''        if structural_retrieval_required(&demands, &candidates) {
            retrieval_trace.stages_invoked.push(RetrievalStage::Structural);
            let seed_paths = ranked_seed_paths(&candidates, self.policy.max_impact_seeds);
            let mut structural_rank = 1_usize;
            for seed in seed_paths {
                for impact in index.impact(&snapshot_id, &seed)? {
                    if candidates.len() >= self.policy.max_candidates
                        && !candidates.contains_key(&impact.path)
                    {
                        break;
                    }
                    let Some(resolved_candidate) =
                        resolve_path_candidate(index, &snapshot_id, &impact.path)?
                    else {
                        continue;
                    };
                    let path = resolved_candidate.path.clone();
                    let candidate = candidates.entry(path).or_insert(resolved_candidate);
                    candidate.structural_rank =
                        min_rank(candidate.structural_rank, structural_rank);
                    candidate
                        .reasons
                        .insert(format!("repository impact: {}", impact.reason));
                    structural_rank = structural_rank.saturating_add(1);
                }
            }
        }

'''
text = replace_between(text, start, end, replacement, "conditional structural retrieval")

text = replace_once(
    text,
    '''        let pack = self.select(workspace_root, index, &snapshot_id, request, ranked)?;

        let final_check = index.search_current(
            workspace_root,
            &SearchQuery {
                text: request.objective.clone(),
                limit: 1,
                min_score_micros: 0,
            },
        )?;
        if final_check.snapshot_id != snapshot_id {
''',
    '''        let pack = self.select(
            workspace_root,
            index,
            &snapshot_id,
            request,
            &demands,
            retrieval_trace,
            ranked,
        )?;

        if index.verified_current_snapshot_id(workspace_root)? != snapshot_id {
''',
    "compile final freshness",
)
text = replace_once(
    text,
    '''        let final_check = index.search_current(
            workspace_root,
            &SearchQuery {
                text: pack.task_id.clone(),
                limit: 1,
                min_score_micros: 0,
            },
        )?;
        if final_check.snapshot_id != pack.repo_snapshot {
''',
    '''        if index.verified_current_snapshot_id(workspace_root)? != pack.repo_snapshot {
''',
    "fidelity final freshness",
)
text = replace_once(
    text,
    '''        snapshot_id: &str,
        request: &ContextRequest,
        ranked: Vec<Candidate>,
''',
    '''        snapshot_id: &str,
        request: &ContextRequest,
        demands: &[EvidenceDemand],
        mut retrieval_trace: RetrievalTrace,
        ranked: Vec<Candidate>,
''',
    "select demand parameters",
)
text = replace_once(
    text,
    '''        for candidate in ranked
            .iter()
            .filter(|candidate| !candidate.required_definitions.is_empty())
        {
            mandatory_paths.insert(candidate.path.clone());
        }

''',
    '''        for candidate in ranked
            .iter()
            .filter(|candidate| !candidate.required_definitions.is_empty())
        {
            mandatory_paths.insert(candidate.path.clone());
        }
        for candidate in &ranked {
            if demands.iter().any(|demand| {
                demand.verification_critical && candidate_covers_demand(candidate, demand)
            }) {
                mandatory_paths.insert(candidate.path.clone());
            }
        }

''',
    "mandatory demand paths",
)

select_start = text.find("    fn select(\n")
if select_start < 0:
    raise SystemExit("select function not found")
select_end = text.find("    fn materialize(\n", select_start)
if select_end < 0:
    raise SystemExit("materialize marker not found")
select = text[select_start:select_end]
mid_start = '''        for semantic_id in &request.required_semantic_ids {
'''
mid_end = '''        // Fail closed rather than ship a pack that silently omits a definition the
'''
new_mid = '''        // Evidence sufficiency, not spare budget, drives selection. A candidate
        // is admitted only when it adds coverage to an unsatisfied demand.
        while !demands_satisfied(demands, &materialized, &selected) {
            if selected.len() >= self.policy.max_items {
                let demand_id = first_unsatisfied_demand(demands, &materialized, &selected)
                    .unwrap_or_else(|| "unknown".to_owned());
                return Err(ContextError::EvidenceDemandUnsatisfied(demand_id));
            }

            let mut best: Option<(&MaterializedCandidate, ContextItem, usize, u64)> = None;
            for candidate in materialized
                .iter()
                .filter(|candidate| !selected.contains_key(&candidate.candidate.path))
            {
                let (marginal, tier, demand_ids) =
                    marginal_demand_value(candidate, demands, &materialized, &selected);
                if marginal == 0 {
                    continue;
                }
                let mut item = candidate.tier(tier, &self.policy)?;
                if !demand_ids.is_empty() {
                    item.selected_reason.push_str("; evidence demand: ");
                    item.selected_reason.push_str(&demand_ids.join(", "));
                }
                let ratio = candidate
                    .candidate
                    .utility_micros
                    .saturating_mul(u64::try_from(marginal).unwrap_or(u64::MAX))
                    .saturating_mul(RRF_SCALE)
                    .checked_div(u64::from(item.token_cost.max(1)))
                    .ok_or_else(|| {
                        ContextError::Arithmetic("marginal utility/token division".to_owned())
                    })?;
                let replace = match &best {
                    None => true,
                    Some((current, _, current_marginal, current_ratio)) => {
                        marginal > *current_marginal
                            || (marginal == *current_marginal && ratio > *current_ratio)
                            || (marginal == *current_marginal
                                && ratio == *current_ratio
                                && candidate.candidate.path < current.candidate.path)
                    }
                };
                if replace {
                    best = Some((candidate, item, marginal, ratio));
                }
            }

            let Some((candidate, item, _, _)) = best else {
                let demand_id = first_unsatisfied_demand(demands, &materialized, &selected)
                    .unwrap_or_else(|| "unknown".to_owned());
                return Err(ContextError::EvidenceDemandUnsatisfied(demand_id));
            };
            if item.token_cost > remaining {
                return Err(ContextError::BudgetTooSmall {
                    required: available
                        .saturating_sub(remaining)
                        .saturating_add(item.token_cost),
                    available,
                });
            }
            remaining -= item.token_cost;
            selected.insert(candidate.candidate.path.clone(), item);
        }

        retrieval_trace.demands_satisfied = demands
            .iter()
            .filter(|demand| demand_satisfied(demand, &materialized, &selected))
            .count();
        retrieval_trace.tier_escalations = selected
            .values()
            .filter(|item| item.tier > ContextTier::Structural)
            .count();
        if retrieval_trace.demands_satisfied != retrieval_trace.demands_total {
            let demand_id = first_unsatisfied_demand(demands, &materialized, &selected)
                .unwrap_or_else(|| "unknown".to_owned());
            return Err(ContextError::EvidenceDemandUnsatisfied(demand_id));
        }

        for semantic_id in &request.required_semantic_ids {
            if !selected.values().any(|item| {
                item.required_semantic_ids.iter().any(|id| id == semantic_id)
            }) {
                return Err(ContextError::MandatoryCoverageUnavailable(semantic_id.clone()));
            }
        }

'''
select = replace_between(select, mid_start, mid_end, new_mid, "demand selection body")
text = text[:select_start] + select + text[select_end:]

text = replace_once(
    text,
    '''            items,
            omitted_high_rank_items,
            source_hashes,
        };
''',
    '''            items,
            omitted_high_rank_items,
            source_hashes,
            evidence_demands: demands.to_vec(),
            retrieval_trace,
        };
''',
    "context pack demand audit",
)

helpers = r'''fn compile_evidence_demands(request: &ContextRequest) -> Vec<EvidenceDemand> {
    let mut demands = Vec::new();
    for (index, symbol) in request.required_symbols.iter().enumerate() {
        demands.push(EvidenceDemand {
            demand_id: format!("exact-definition:{index}:{symbol}"),
            kind: EvidenceDemandKind::ExactDefinition,
            target: EvidenceDemandTarget::Symbol(symbol.clone()),
            minimum_tier: ContextTier::Expanded,
            required_provenance: EvidenceProvenance::ExactSource,
            minimum_coverage: 1,
            expansion_policy: EvidenceExpansionPolicy::ExactDefinition,
            importance_milli: 1000,
            verification_critical: true,
        });
    }
    for (index, semantic_id) in request.required_semantic_ids.iter().enumerate() {
        demands.push(EvidenceDemand {
            demand_id: format!("requirement-context:{index}:{semantic_id}"),
            kind: EvidenceDemandKind::RequirementContext,
            target: EvidenceDemandTarget::SemanticId(semantic_id.clone()),
            minimum_tier: ContextTier::Structural,
            required_provenance: EvidenceProvenance::IndexedSource,
            minimum_coverage: 1,
            expansion_policy: EvidenceExpansionPolicy::Never,
            importance_milli: 950,
            verification_critical: true,
        });
    }
    for (index, hint) in request.runtime_hints.iter().enumerate() {
        if hint.score_milli == 0 {
            continue;
        }
        demands.push(EvidenceDemand {
            demand_id: format!("runtime-evidence:{index}:{}", hint.path),
            kind: EvidenceDemandKind::RuntimeEvidence,
            target: EvidenceDemandTarget::Path(hint.path.clone()),
            minimum_tier: ContextTier::SourceSpan,
            required_provenance: EvidenceProvenance::RuntimeObserved,
            minimum_coverage: 1,
            expansion_policy: EvidenceExpansionPolicy::BoundedSourceSpan,
            importance_milli: hint.score_milli,
            verification_critical: false,
        });
    }

    // Exact/semantic/runtime facts are allowed to terminate discovery. Only a
    // task with no such deterministic demand receives broad objective demands.
    if demands.is_empty() {
        let objective_lower = request.objective.to_ascii_lowercase();
        let support_coverage = if objective_lower.contains("test") { 2 } else { 1 };
        demands.push(EvidenceDemand {
            demand_id: "objective:edit-target".to_owned(),
            kind: EvidenceDemandKind::EditTarget,
            target: EvidenceDemandTarget::Objective,
            minimum_tier: ContextTier::SourceSpan,
            required_provenance: EvidenceProvenance::ExactSource,
            minimum_coverage: 1,
            expansion_policy: EvidenceExpansionPolicy::BoundedSourceSpan,
            importance_milli: 1000,
            verification_critical: true,
        });
        demands.push(EvidenceDemand {
            demand_id: "objective:supporting-context".to_owned(),
            kind: EvidenceDemandKind::SupportingContext,
            target: EvidenceDemandTarget::Objective,
            minimum_tier: ContextTier::Structural,
            required_provenance: EvidenceProvenance::IndexedSource,
            minimum_coverage: support_coverage,
            expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,
            importance_milli: 700,
            verification_critical: false,
        });
    }
    demands
}

fn demand_needs_lexical(demand: &EvidenceDemand) -> bool {
    matches!(demand.target, EvidenceDemandTarget::Objective)
}

fn demand_requires_structural(demand: &EvidenceDemand) -> bool {
    matches!(
        demand.kind,
        EvidenceDemandKind::ChangeImpact | EvidenceDemandKind::SupportingContext
    ) && demand.expansion_policy == EvidenceExpansionPolicy::BoundedNeighborhood
}

fn structural_retrieval_required(
    demands: &[EvidenceDemand],
    candidates: &BTreeMap<String, Candidate>,
) -> bool {
    demands.iter().any(|demand| {
        demand_requires_structural(demand)
            && usize::from(demand.minimum_coverage)
                > candidates
                    .values()
                    .filter(|candidate| candidate_covers_demand(candidate, demand))
                    .count()
    })
}

fn candidate_covers_demand(candidate: &Candidate, demand: &EvidenceDemand) -> bool {
    match &demand.target {
        EvidenceDemandTarget::Symbol(symbol) => candidate.required_symbols.contains(symbol),
        EvidenceDemandTarget::SemanticId(semantic_id) => {
            candidate.required_semantic_ids.contains(semantic_id)
        }
        EvidenceDemandTarget::Path(path) => candidate.path == *path,
        EvidenceDemandTarget::Objective => {
            candidate.lexical_rank.is_some() || candidate.structural_rank.is_some()
        }
    }
}

fn demand_satisfied(
    demand: &EvidenceDemand,
    materialized: &[MaterializedCandidate],
    selected: &BTreeMap<String, ContextItem>,
) -> bool {
    materialized
        .iter()
        .filter(|candidate| {
            selected.contains_key(&candidate.candidate.path)
                && candidate_covers_demand(&candidate.candidate, demand)
        })
        .count()
        >= usize::from(demand.minimum_coverage)
}

fn demands_satisfied(
    demands: &[EvidenceDemand],
    materialized: &[MaterializedCandidate],
    selected: &BTreeMap<String, ContextItem>,
) -> bool {
    demands
        .iter()
        .all(|demand| demand_satisfied(demand, materialized, selected))
}

fn first_unsatisfied_demand(
    demands: &[EvidenceDemand],
    materialized: &[MaterializedCandidate],
    selected: &BTreeMap<String, ContextItem>,
) -> Option<String> {
    demands
        .iter()
        .find(|demand| !demand_satisfied(demand, materialized, selected))
        .map(|demand| demand.demand_id.clone())
}

fn marginal_demand_value(
    candidate: &MaterializedCandidate,
    demands: &[EvidenceDemand],
    materialized: &[MaterializedCandidate],
    selected: &BTreeMap<String, ContextItem>,
) -> (usize, ContextTier, Vec<String>) {
    let mut marginal = 0_usize;
    let mut tier = ContextTier::Identifier;
    let mut demand_ids = Vec::new();
    for demand in demands {
        if demand_satisfied(demand, materialized, selected)
            || !candidate_covers_demand(&candidate.candidate, demand)
        {
            continue;
        }
        marginal = marginal.saturating_add(usize::from(demand.importance_milli.max(1)));
        tier = tier.max(demand.minimum_tier);
        demand_ids.push(demand.demand_id.clone());
    }
    (marginal, tier.max(ContextTier::Structural), demand_ids)
}

'''
text = replace_once(
    text,
    '''fn resolve_path_candidate(
''',
    helpers + '''fn resolve_path_candidate(
''',
    "context demand helpers",
)
write(path, text)


# ---------------------------------------------------------------------------
# Context regression tests: budget ceiling and retrieval-stage sufficiency.
# ---------------------------------------------------------------------------
path = "crates/aer-context/tests/context_bench.rs"
text = read(path)
marker = '''#[test]
fn stale_workspace_is_rejected_before_a_pack_can_be_reused() {
'''
tests = '''#[test]
fn satisfied_evidence_is_invariant_to_a_larger_budget() {
    let (fixture, index, _) = indexed_fixture();
    let engine = ContextEngine::new(ContextPolicy::default()).expect("context engine");
    let low = ContextRequest::new(
        "task-budget-ceiling",
        "fix expired authentication token verification and its tests",
        1,
        1200,
    );
    let high = ContextRequest::new(
        "task-budget-ceiling",
        "fix expired authentication token verification and its tests",
        1,
        2400,
    );

    let low_pack = engine
        .compile(&fixture.repo, &index, &low)
        .expect("low-budget pack");
    let high_pack = engine
        .compile(&fixture.repo, &index, &high)
        .expect("high-budget pack");
    let semantic_payload = |pack: &aer_context::ContextPack| {
        pack.items
            .iter()
            .map(|item| {
                (
                    item.path.clone(),
                    item.tier,
                    item.source_ref.clone(),
                    item.rendered_text.clone(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(semantic_payload(&low_pack), semantic_payload(&high_pack));
    assert_eq!(low_pack.retrieval_trace, high_pack.retrieval_trace);
    assert!(low_pack.total_token_cost() < high.input_token_budget);
}

#[test]
fn exact_definition_demand_stops_before_lexical_or_structural_retrieval() {
    let (fixture, index, _) = indexed_fixture();
    let engine = ContextEngine::new(ContextPolicy::default()).expect("context engine");
    let mut request = ContextRequest::new("task-exact", "inspect `verify_token`", 1, 1200);
    request.required_symbols.push("verify_token".to_owned());
    let pack = engine
        .compile(&fixture.repo, &index, &request)
        .expect("exact pack");
    assert_eq!(pack.items.len(), 1);
    assert_eq!(
        pack.retrieval_trace.stages_invoked,
        vec![aer_context::RetrievalStage::Exact]
    );
}

#[test]
fn stale_workspace_is_rejected_before_a_pack_can_be_reused() {
'''
text = replace_once(text, marker, tests, "context sufficiency tests")
write(path, text)

print("Context Economy V2 source transformations applied successfully")
