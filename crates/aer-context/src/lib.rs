//! Bounded, provenance-preserving Context Pack compilation.
//!
//! The context engine consumes derived repository intelligence and produces the smallest useful
//! source-faithful pack it can under an explicit token budget. It is not an authority store and it
//! never mutates project semantics.

mod model;

pub use model::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use aer_contracts::embedded::EmbeddedContractRegistry;
use aer_domain::contracts::CoreContract;
use aer_repo::{RepositoryIndex, SearchHit, SearchQuery};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const RRF_SCALE: u64 = 1_000_000;

pub struct ContextEngine {
    policy: ContextPolicy,
    contracts: EmbeddedContractRegistry,
}

impl ContextEngine {
    pub fn new(policy: ContextPolicy) -> Result<Self, ContextError> {
        policy.validate()?;
        let contracts = EmbeddedContractRegistry::load()
            .map_err(|error| ContextError::ContractRegistry(error.to_string()))?;
        Ok(Self { policy, contracts })
    }

    pub fn compile(
        &self,
        workspace_root: impl AsRef<Path>,
        index: &RepositoryIndex,
        request: &ContextRequest,
    ) -> Result<ContextPack, ContextError> {
        validate_request(request, &self.policy)?;
        let workspace_root = workspace_root.as_ref();
        let lexical = index.search_current(
            workspace_root,
            &SearchQuery {
                text: request.objective.clone(),
                limit: self.policy.max_candidates.min(64),
                min_score_micros: 100_000,
            },
        )?;
        let snapshot_id = lexical.snapshot_id.clone();
        let mut candidates = BTreeMap::<String, Candidate>::new();

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

        for semantic_id in &request.required_semantic_ids {
            let links = index.semantic_links(&snapshot_id, semantic_id)?;
            if links.is_empty() {
                return Err(ContextError::MandatoryCoverageUnavailable(
                    semantic_id.clone(),
                ));
            }
            let mut resolved = false;
            for (rank, link) in links.into_iter().enumerate() {
                let Some(hit) = resolve_path_hit(index, &snapshot_id, &link.target_path)? else {
                    continue;
                };
                resolved = true;
                let candidate = candidates
                    .entry(hit.path.clone())
                    .or_insert_with(|| Candidate::from_hit(&hit));
                candidate.merge_hit(&hit);
                candidate.semantic_rank = min_rank(candidate.semantic_rank, rank + 1);
                candidate.required_semantic_ids.insert(semantic_id.clone());
                candidate
                    .reasons
                    .insert("Engineering IR semantic link".to_owned());
            }
            if !resolved {
                return Err(ContextError::MandatoryCoverageUnavailable(
                    semantic_id.clone(),
                ));
            }
        }

        for hint in &request.runtime_hints {
            if hint.score_milli == 0 {
                continue;
            }
            let Some(hit) = resolve_path_hit(index, &snapshot_id, &hint.path)? else {
                continue;
            };
            let candidate = candidates
                .entry(hit.path.clone())
                .or_insert_with(|| Candidate::from_hit(&hit));
            candidate.merge_hit(&hit);
            candidate.runtime_rank =
                min_rank(candidate.runtime_rank, runtime_rank(hint.score_milli));
            candidate
                .reasons
                .insert(format!("runtime hint: {}", hint.reason));
        }

        let seed_paths = ranked_seed_paths(&candidates, self.policy.max_impact_seeds);
        let mut structural_rank = 1_usize;
        for seed in seed_paths {
            for impact in index.impact(&snapshot_id, &seed)? {
                if candidates.len() >= self.policy.max_candidates
                    && !candidates.contains_key(&impact.path)
                {
                    break;
                }
                let Some(hit) = resolve_path_hit(index, &snapshot_id, &impact.path)? else {
                    continue;
                };
                let candidate = candidates
                    .entry(hit.path.clone())
                    .or_insert_with(|| Candidate::from_hit(&hit));
                candidate.merge_hit(&hit);
                candidate.structural_rank = min_rank(candidate.structural_rank, structural_rank);
                candidate
                    .reasons
                    .insert(format!("repository impact: {}", impact.reason));
                structural_rank = structural_rank.saturating_add(1);
            }
        }

        for candidate in candidates.values_mut() {
            candidate.utility_micros = utility(candidate, &self.policy)?;
        }
        let mut ranked = candidates.into_values().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .utility_micros
                .cmp(&left.utility_micros)
                .then_with(|| left.path.cmp(&right.path))
        });
        ranked.truncate(self.policy.max_candidates);

        let pack = self.select(workspace_root, index, &snapshot_id, request, ranked)?;

        let final_check = index.search_current(
            workspace_root,
            &SearchQuery {
                text: request.objective.clone(),
                limit: 1,
                min_score_micros: 0,
            },
        )?;
        if final_check.snapshot_id != snapshot_id {
            return Err(ContextError::Fidelity(
                "repository snapshot changed while the Context Pack was compiled".to_owned(),
            ));
        }
        self.validate_contract(&pack)?;
        Ok(pack)
    }

    pub fn verify_fidelity(
        &self,
        workspace_root: impl AsRef<Path>,
        index: &RepositoryIndex,
        pack: &ContextPack,
    ) -> Result<(), ContextError> {
        self.validate_contract(pack)?;
        let workspace_root = workspace_root.as_ref();
        if pack.total_token_cost() > pack.input_token_budget {
            return Err(ContextError::Fidelity(format!(
                "pack token cost {} exceeds budget {}",
                pack.total_token_cost(),
                pack.input_token_budget
            )));
        }
        let final_check = index.search_current(
            workspace_root,
            &SearchQuery {
                text: pack.task_id.clone(),
                limit: 1,
                min_score_micros: 0,
            },
        )?;
        if final_check.snapshot_id != pack.repo_snapshot {
            return Err(ContextError::Fidelity(
                "pack repository snapshot is no longer current".to_owned(),
            ));
        }
        for item in &pack.items {
            let bytes = read_source(workspace_root, &item.path, self.policy.max_source_bytes)?;
            if sha256(&bytes) != item.content_hash {
                return Err(ContextError::SourceHashMismatch {
                    path: item.path.clone(),
                });
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| {
                ContextError::Fidelity(format!("selected source is not UTF-8: {}", item.path))
            })?;
            for segment in &item.segments {
                let excerpt = extract_lines(text, segment.start_line, segment.end_line)?;
                if sha256(excerpt.as_bytes()) != segment.sha256 {
                    return Err(ContextError::Fidelity(format!(
                        "source segment changed: {}#L{}-L{}",
                        item.path, segment.start_line, segment.end_line
                    )));
                }
            }
        }
        Ok(())
    }

    fn select(
        &self,
        workspace_root: &Path,
        index: &RepositoryIndex,
        snapshot_id: &str,
        request: &ContextRequest,
        ranked: Vec<Candidate>,
    ) -> Result<ContextPack, ContextError> {
        let available = request.input_token_budget;
        if available <= self.policy.base_pack_token_overhead {
            return Err(ContextError::BudgetTooSmall {
                required: self.policy.base_pack_token_overhead.saturating_add(1),
                available,
            });
        }
        let mut remaining = available - self.policy.base_pack_token_overhead;
        let consideration_limit = self
            .policy
            .max_items
            .saturating_mul(3)
            .saturating_add(request.required_semantic_ids.len())
            .min(ranked.len());
        let mut materialized = Vec::with_capacity(consideration_limit);
        let mut mandatory_paths = BTreeSet::new();

        for semantic_id in &request.required_semantic_ids {
            if let Some(candidate) = ranked
                .iter()
                .find(|candidate| candidate.required_semantic_ids.contains(semantic_id))
            {
                mandatory_paths.insert(candidate.path.clone());
            }
        }

        for candidate in ranked.iter().take(consideration_limit).chain(
            ranked
                .iter()
                .filter(|candidate| mandatory_paths.contains(&candidate.path)),
        ) {
            if materialized
                .iter()
                .any(|existing: &MaterializedCandidate| existing.candidate.path == candidate.path)
            {
                continue;
            }
            match self.materialize(workspace_root, index, snapshot_id, candidate) {
                Ok(value) => materialized.push(value),
                Err(
                    ContextError::SourceTooLarge { .. } | ContextError::SourceNotRetrievable(_),
                ) if !mandatory_paths.contains(&candidate.path) => {}
                Err(error) => return Err(error),
            }
        }

        let mut selected = BTreeMap::<String, ContextItem>::new();
        for semantic_id in &request.required_semantic_ids {
            let candidate = materialized
                .iter()
                .filter(|candidate| {
                    candidate
                        .candidate
                        .required_semantic_ids
                        .contains(semantic_id)
                })
                .max_by(|left, right| {
                    left.candidate
                        .utility_micros
                        .cmp(&right.candidate.utility_micros)
                        .then_with(|| right.candidate.path.cmp(&left.candidate.path))
                })
                .ok_or_else(|| ContextError::MandatoryCoverageUnavailable(semantic_id.clone()))?;
            if selected.contains_key(&candidate.candidate.path) {
                continue;
            }
            let item = candidate.tier(ContextTier::Structural, &self.policy)?;
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

        let mut general = materialized
            .iter()
            .filter(|candidate| !selected.contains_key(&candidate.candidate.path))
            .map(|candidate| {
                let item = candidate.tier(ContextTier::Structural, &self.policy)?;
                let ratio = candidate
                    .candidate
                    .utility_micros
                    .saturating_mul(RRF_SCALE)
                    .checked_div(u64::from(item.token_cost.max(1)))
                    .ok_or_else(|| ContextError::Arithmetic("utility/token division".to_owned()))?;
                Ok((candidate, item, ratio))
            })
            .collect::<Result<Vec<_>, ContextError>>()?;
        general.sort_by(|left, right| {
            right
                .2
                .cmp(&left.2)
                .then_with(|| {
                    right
                        .0
                        .candidate
                        .utility_micros
                        .cmp(&left.0.candidate.utility_micros)
                })
                .then_with(|| left.0.candidate.path.cmp(&right.0.candidate.path))
        });

        for (candidate, item, _) in general {
            if selected.len() >= self.policy.max_items {
                break;
            }
            if item.token_cost <= remaining {
                remaining -= item.token_cost;
                selected.insert(candidate.candidate.path.clone(), item);
            }
        }

        for tier in [ContextTier::SourceSpan, ContextTier::Expanded] {
            let mut paths = selected.keys().cloned().collect::<Vec<_>>();
            paths.sort_by(|left, right| {
                let left_utility = materialized
                    .iter()
                    .find(|candidate| candidate.candidate.path == *left)
                    .map_or(0, |candidate| candidate.candidate.utility_micros);
                let right_utility = materialized
                    .iter()
                    .find(|candidate| candidate.candidate.path == *right)
                    .map_or(0, |candidate| candidate.candidate.utility_micros);
                right_utility
                    .cmp(&left_utility)
                    .then_with(|| left.cmp(right))
            });
            for path in paths {
                let Some(candidate) = materialized
                    .iter()
                    .find(|candidate| candidate.candidate.path == path)
                else {
                    continue;
                };
                let richer = candidate.tier(tier, &self.policy)?;
                let current = selected
                    .get(&path)
                    .expect("selected path originated from materialized candidate");
                if richer.token_cost <= current.token_cost {
                    continue;
                }
                let increment = richer.token_cost - current.token_cost;
                if increment <= remaining {
                    remaining -= increment;
                    selected.insert(path, richer);
                }
            }
        }

        let selected_paths = selected.keys().cloned().collect::<BTreeSet<_>>();
        let mut items = selected.into_values().collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .utility_micros
                .cmp(&left.utility_micros)
                .then_with(|| left.path.cmp(&right.path))
        });
        let omitted_high_rank_items = ranked
            .iter()
            .filter(|candidate| !selected_paths.contains(&candidate.path))
            .take(self.policy.omitted_high_rank_limit)
            .map(|candidate| candidate.path.clone())
            .collect::<Vec<_>>();
        let source_hashes = items
            .iter()
            .map(|item| item.content_hash.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let mut pack = ContextPack {
            schema_version: 1,
            pack_id: "pending".to_owned(),
            task_id: request.task_id.clone(),
            engineering_ir_version: request.engineering_ir_version,
            repo_snapshot: snapshot_id.to_owned(),
            policy_version: self.policy.version.clone(),
            input_token_budget: request.input_token_budget,
            items,
            omitted_high_rank_items,
            source_hashes,
        };
        let identity = manifest_value(&pack);
        pack.pack_id = format!(
            "context-pack:{}",
            sha256_digest(&serde_json::to_vec(&identity)?)
        );
        Ok(pack)
    }

    fn materialize(
        &self,
        workspace_root: &Path,
        index: &RepositoryIndex,
        snapshot_id: &str,
        candidate: &Candidate,
    ) -> Result<MaterializedCandidate, ContextError> {
        let bytes = read_source(
            workspace_root,
            &candidate.path,
            self.policy.max_source_bytes,
        )?;
        if sha256(&bytes) != candidate.content_hash {
            return Err(ContextError::SourceHashMismatch {
                path: candidate.path.clone(),
            });
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| ContextError::SourceNotRetrievable(candidate.path.clone()))?
            .to_owned();
        let line_count = u32::try_from(text.lines().count())
            .unwrap_or(u32::MAX)
            .max(1);
        let mut symbol_span = None;
        for symbol_name in &candidate.matched_symbols {
            for symbol in index.symbols(snapshot_id, symbol_name)? {
                if symbol.path == candidate.path {
                    symbol_span = Some((symbol.start_line.max(1), symbol.end_line.max(1)));
                    break;
                }
            }
            if symbol_span.is_some() {
                break;
            }
        }
        Ok(MaterializedCandidate {
            candidate: candidate.clone(),
            text,
            line_count,
            symbol_span,
        })
    }

    fn validate_contract(&self, pack: &ContextPack) -> Result<(), ContextError> {
        let instance = manifest_value(pack);
        self.contracts
            .validate_current(CoreContract::ContextPack, &instance)
            .map_err(|error| ContextError::ContractValidation(error.issues))
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    path: String,
    content_hash: String,
    anchor_line: Option<u32>,
    matched_terms: BTreeSet<String>,
    matched_symbols: BTreeSet<String>,
    required_semantic_ids: BTreeSet<String>,
    reasons: BTreeSet<String>,
    lexical_rank: Option<usize>,
    semantic_rank: Option<usize>,
    structural_rank: Option<usize>,
    runtime_rank: Option<usize>,
    utility_micros: u64,
}

impl Candidate {
    fn from_hit(hit: &SearchHit) -> Self {
        let mut candidate = Self {
            path: hit.path.clone(),
            content_hash: hit.content_sha256.clone(),
            anchor_line: hit.anchor_line,
            matched_terms: BTreeSet::new(),
            matched_symbols: BTreeSet::new(),
            required_semantic_ids: BTreeSet::new(),
            reasons: BTreeSet::new(),
            lexical_rank: None,
            semantic_rank: None,
            structural_rank: None,
            runtime_rank: None,
            utility_micros: 0,
        };
        candidate.merge_hit(hit);
        candidate
    }

    fn merge_hit(&mut self, hit: &SearchHit) {
        self.content_hash.clone_from(&hit.content_sha256);
        self.anchor_line = match (self.anchor_line, hit.anchor_line) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (left, right) => left.or(right),
        };
        self.matched_terms.extend(hit.matched_terms.iter().cloned());
        self.matched_symbols
            .extend(hit.matched_symbols.iter().cloned());
    }
}

struct MaterializedCandidate {
    candidate: Candidate,
    text: String,
    line_count: u32,
    symbol_span: Option<(u32, u32)>,
}

impl MaterializedCandidate {
    fn tier(&self, tier: ContextTier, policy: &ContextPolicy) -> Result<ContextItem, ContextError> {
        let (segments, body) = match tier {
            ContextTier::Identifier => (Vec::new(), format!("path: {}", self.candidate.path)),
            ContextTier::Structural => {
                let line = self
                    .candidate
                    .anchor_line
                    .unwrap_or(1)
                    .clamp(1, self.line_count);
                let segment = source_segment(&self.text, line, line)?;
                (
                    vec![segment.clone()],
                    format!(
                        "{}#L{}\n{}",
                        self.candidate.path,
                        line,
                        extract_lines(&self.text, line, line)?
                    ),
                )
            }
            ContextTier::SourceSpan => {
                let (start, end) = bounded_source_span(
                    self.line_count,
                    self.candidate.anchor_line.unwrap_or(1),
                    self.symbol_span,
                    policy.max_span_lines,
                );
                let segment = source_segment(&self.text, start, end)?;
                (
                    vec![segment.clone()],
                    format!(
                        "{}#L{}-L{}\n{}",
                        self.candidate.path,
                        start,
                        end,
                        extract_lines(&self.text, start, end)?
                    ),
                )
            }
            ContextTier::Expanded => {
                let (base_start, base_end) = bounded_source_span(
                    self.line_count,
                    self.candidate.anchor_line.unwrap_or(1),
                    self.symbol_span,
                    policy.max_span_lines,
                );
                let desired = policy.max_tier3_lines.min(self.line_count);
                let extra =
                    desired.saturating_sub(base_end.saturating_sub(base_start).saturating_add(1));
                let before = extra / 2;
                let mut start = base_start.saturating_sub(before).max(1);
                let mut end = start
                    .saturating_add(desired.saturating_sub(1))
                    .min(self.line_count);
                if end.saturating_sub(start).saturating_add(1) < desired {
                    start = end.saturating_sub(desired.saturating_sub(1)).max(1);
                    end = start
                        .saturating_add(desired.saturating_sub(1))
                        .min(self.line_count);
                }
                let segment = source_segment(&self.text, start, end)?;
                (
                    vec![segment.clone()],
                    format!(
                        "{}#L{}-L{}\n{}",
                        self.candidate.path,
                        start,
                        end,
                        extract_lines(&self.text, start, end)?
                    ),
                )
            }
        };
        let token_cost = estimate_tokens(&body)
            .saturating_add(policy.per_item_token_overhead)
            .max(1);
        let source_ref = if let Some(segment) = segments.first() {
            format!(
                "{}#L{}-L{}",
                self.candidate.path, segment.start_line, segment.end_line
            )
        } else {
            self.candidate.path.clone()
        };
        let mut required_semantic_ids = self
            .candidate
            .required_semantic_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        required_semantic_ids.sort();
        Ok(ContextItem {
            id: context_item_id(
                &self.candidate.content_hash,
                &source_ref,
                tier,
                segments.first().map(|segment| segment.sha256.as_str()),
            ),
            source_type: "repository_source".to_owned(),
            source_ref,
            content_hash: self.candidate.content_hash.clone(),
            token_cost,
            tier,
            selected_reason: self
                .candidate
                .reasons
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
            path: self.candidate.path.clone(),
            utility_micros: self.candidate.utility_micros,
            required_semantic_ids,
            segments,
            rendered_text: body,
        })
    }
}

fn validate_request(request: &ContextRequest, policy: &ContextPolicy) -> Result<(), ContextError> {
    if request.task_id.trim().is_empty() {
        return Err(ContextError::InvalidRequest("task_id is empty".to_owned()));
    }
    if request.objective.trim().is_empty() {
        return Err(ContextError::InvalidRequest(
            "objective is empty".to_owned(),
        ));
    }
    if request.engineering_ir_version == 0 {
        return Err(ContextError::InvalidRequest(
            "engineering_ir_version must be nonzero".to_owned(),
        ));
    }
    if request.input_token_budget == 0 {
        return Err(ContextError::InvalidRequest(
            "input_token_budget must be nonzero".to_owned(),
        ));
    }
    if request.required_semantic_ids.len() > policy.max_semantic_ids {
        return Err(ContextError::InvalidRequest(format!(
            "semantic id count exceeds {}",
            policy.max_semantic_ids
        )));
    }
    if request.runtime_hints.len() > policy.max_runtime_hints {
        return Err(ContextError::InvalidRequest(format!(
            "runtime hint count exceeds {}",
            policy.max_runtime_hints
        )));
    }
    if request
        .required_semantic_ids
        .iter()
        .any(|id| id.trim().is_empty())
    {
        return Err(ContextError::InvalidRequest(
            "semantic ids cannot be empty".to_owned(),
        ));
    }
    if request
        .runtime_hints
        .iter()
        .any(|hint| hint.path.trim().is_empty() || hint.score_milli > 1000)
    {
        return Err(ContextError::InvalidRequest(
            "runtime hints require a path and score_milli <= 1000".to_owned(),
        ));
    }
    Ok(())
}

fn resolve_path_hit(
    index: &RepositoryIndex,
    snapshot_id: &str,
    path: &str,
) -> Result<Option<SearchHit>, ContextError> {
    let query = SearchQuery {
        text: path.to_owned(),
        limit: 12,
        min_score_micros: 0,
    };
    Ok(index
        .search(snapshot_id, &query)?
        .hits
        .into_iter()
        .find(|hit| hit.path == path))
}

fn ranked_seed_paths(candidates: &BTreeMap<String, Candidate>, limit: usize) -> Vec<String> {
    let mut seeds = candidates.values().cloned().collect::<Vec<_>>();
    seeds.sort_by(|left, right| {
        candidate_pre_score(right)
            .cmp(&candidate_pre_score(left))
            .then_with(|| left.path.cmp(&right.path))
    });
    seeds
        .into_iter()
        .take(limit)
        .map(|candidate| candidate.path)
        .collect()
}

fn candidate_pre_score(candidate: &Candidate) -> u64 {
    rank_score(candidate.semantic_rank, 1400, 60)
        .saturating_add(rank_score(candidate.lexical_rank, 1000, 60))
        .saturating_add(rank_score(candidate.runtime_rank, 1200, 60))
}

fn utility(candidate: &Candidate, policy: &ContextPolicy) -> Result<u64, ContextError> {
    let mut value = 0_u64;
    for (rank, weight) in [
        (candidate.lexical_rank, policy.lexical_weight),
        (candidate.semantic_rank, policy.semantic_weight),
        (candidate.structural_rank, policy.structural_weight),
        (candidate.runtime_rank, policy.runtime_weight),
    ] {
        value = value
            .checked_add(rank_score(rank, weight, policy.rrf_k))
            .ok_or_else(|| ContextError::Arithmetic("RRF utility overflow".to_owned()))?;
    }
    if !candidate.required_semantic_ids.is_empty() {
        value = value.saturating_add(RRF_SCALE);
    }
    Ok(value.max(1))
}

fn rank_score(rank: Option<usize>, weight: u32, k: u32) -> u64 {
    let Some(rank) = rank else {
        return 0;
    };
    let denominator = u64::from(k).saturating_add(u64::try_from(rank).unwrap_or(u64::MAX));
    u64::from(weight)
        .saturating_mul(RRF_SCALE)
        .checked_div(denominator.max(1))
        .unwrap_or(0)
}

fn min_rank(existing: Option<usize>, new_rank: usize) -> Option<usize> {
    Some(existing.map_or(new_rank, |rank| rank.min(new_rank)))
}

fn runtime_rank(score_milli: u16) -> usize {
    let inverse = 1000_u16.saturating_sub(score_milli);
    usize::from(inverse / 50).saturating_add(1)
}

fn bounded_source_span(
    line_count: u32,
    anchor_line: u32,
    symbol_span: Option<(u32, u32)>,
    max_lines: u32,
) -> (u32, u32) {
    let anchor = anchor_line.clamp(1, line_count.max(1));
    let (mut start, mut end) = symbol_span.unwrap_or((anchor, anchor));
    start = start.clamp(1, line_count.max(1));
    end = end.clamp(start, line_count.max(1));
    let current = end.saturating_sub(start).saturating_add(1);
    if current > max_lines {
        let before = max_lines / 2;
        start = anchor.saturating_sub(before).max(1);
        end = start
            .saturating_add(max_lines.saturating_sub(1))
            .min(line_count);
        if end.saturating_sub(start).saturating_add(1) < max_lines {
            start = end.saturating_sub(max_lines.saturating_sub(1)).max(1);
        }
    } else if current < max_lines {
        let extra = max_lines.saturating_sub(current);
        start = start.saturating_sub(extra / 2).max(1);
        end = start
            .saturating_add(max_lines.saturating_sub(1))
            .min(line_count);
        if end.saturating_sub(start).saturating_add(1) < max_lines {
            start = end.saturating_sub(max_lines.saturating_sub(1)).max(1);
        }
    }
    (start, end)
}

fn source_segment(
    text: &str,
    start_line: u32,
    end_line: u32,
) -> Result<SourceSegment, ContextError> {
    let excerpt = extract_lines(text, start_line, end_line)?;
    Ok(SourceSegment {
        start_line,
        end_line,
        sha256: sha256(excerpt.as_bytes()),
    })
}

fn extract_lines(text: &str, start_line: u32, end_line: u32) -> Result<String, ContextError> {
    if start_line == 0 || end_line < start_line {
        return Err(ContextError::Fidelity(format!(
            "invalid source line range {start_line}-{end_line}"
        )));
    }
    let start = usize::try_from(start_line - 1)
        .map_err(|_| ContextError::Arithmetic("line start conversion".to_owned()))?;
    let count = usize::try_from(end_line - start_line + 1)
        .map_err(|_| ContextError::Arithmetic("line count conversion".to_owned()))?;
    let lines = text.lines().skip(start).take(count).collect::<Vec<_>>();
    if lines.is_empty() {
        return Err(ContextError::Fidelity(format!(
            "source line range is outside the file: {start_line}-{end_line}"
        )));
    }
    Ok(lines.join("\n"))
}

fn read_source(workspace_root: &Path, path: &str, max_bytes: u64) -> Result<Vec<u8>, ContextError> {
    let source = workspace_root.join(path);
    let metadata = fs::metadata(&source)?;
    if metadata.len() > max_bytes {
        return Err(ContextError::SourceTooLarge {
            path: path.to_owned(),
            bytes: metadata.len(),
        });
    }
    Ok(fs::read(source)?)
}

#[must_use]
pub fn estimate_tokens(text: &str) -> u32 {
    u32::try_from(text.chars().count()).unwrap_or(u32::MAX)
}

#[must_use]
pub fn evaluate_context_pack(
    pack: &ContextPack,
    relevant_paths: &[String],
    naive_token_cost: u64,
) -> ContextBenchMetrics {
    let selected = pack
        .items
        .iter()
        .map(|item| item.path.as_str())
        .collect::<BTreeSet<_>>();
    let relevant_selected = relevant_paths
        .iter()
        .filter(|path| selected.contains(path.as_str()))
        .count();
    let relevant_total = relevant_paths.len();
    let recall_milli = relevant_selected
        .saturating_mul(1000)
        .checked_div(relevant_total)
        .map_or(1000, |value| u16::try_from(value).unwrap_or(1000));
    let selected_tokens = u64::from(pack.total_token_cost()).max(1);
    let naive_tokens = naive_token_cost.max(1);
    let selected_yield_micros = u64::try_from(relevant_selected)
        .unwrap_or(u64::MAX)
        .saturating_mul(RRF_SCALE)
        / selected_tokens;
    let naive_yield_micros = u64::try_from(relevant_total)
        .unwrap_or(u64::MAX)
        .saturating_mul(RRF_SCALE)
        / naive_tokens;
    ContextBenchMetrics {
        relevant_total,
        relevant_selected,
        recall_milli,
        selected_tokens,
        naive_tokens,
        selected_yield_micros,
        naive_yield_micros,
    }
}

fn manifest_value(pack: &ContextPack) -> Value {
    let items = pack
        .items
        .iter()
        .map(|item| {
            json!({
                "id": item.id,
                "source_type": item.source_type,
                "source_ref": item.source_ref,
                "content_hash": item.content_hash,
                "token_cost": item.token_cost,
                "tier": item.tier.as_u8(),
                "provenance": {
                    "repo_snapshot": pack.repo_snapshot,
                    "path": item.path,
                    "required_semantic_ids": item.required_semantic_ids,
                    "segments": item.segments.iter().map(|segment| json!({
                        "start_line": segment.start_line,
                        "end_line": segment.end_line,
                        "sha256": segment.sha256,
                    })).collect::<Vec<_>>()
                },
                "selected_reason": item.selected_reason,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_version": pack.schema_version,
        "pack_id": pack.pack_id,
        "task_id": pack.task_id,
        "engineering_ir_version": pack.engineering_ir_version,
        "repo_snapshot": pack.repo_snapshot,
        "policy_version": pack.policy_version,
        "budget": {"input_tokens": pack.input_token_budget},
        "items": items,
        "omitted_high_rank_items": pack.omitted_high_rank_items,
        "source_hashes": pack.source_hashes,
    })
}

fn context_item_id(
    content_hash: &str,
    source_ref: &str,
    tier: ContextTier,
    span_hash: Option<&str>,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"aer-context-item-v1\0");
    digest.update(content_hash.as_bytes());
    digest.update(b"\0");
    digest.update(source_ref.as_bytes());
    digest.update(b"\0");
    digest.update([tier.as_u8()]);
    if let Some(span_hash) = span_hash {
        digest.update(b"\0");
        digest.update(span_hash.as_bytes());
    }
    format!("context-item:{}", hex(digest.finalize().as_ref()))
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{}", hex(Sha256::digest(bytes).as_ref()))
}

fn sha256_digest(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).as_ref())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 15) as usize] as char);
    }
    output
}

impl From<serde_json::Error> for ContextError {
    fn from(value: serde_json::Error) -> Self {
        Self::Fidelity(format!("Context Pack serialization failed: {value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_span_never_exceeds_requested_lines() {
        let (start, end) = bounded_source_span(1000, 500, Some((1, 900)), 80);
        assert!(start >= 1);
        assert!(end <= 1000);
        assert!(end - start < 80);
        assert!(start <= 500 && end >= 500);
    }

    #[test]
    fn conservative_estimator_is_deterministic() {
        assert_eq!(estimate_tokens("abc λ"), 5);
        assert_eq!(estimate_tokens("abc λ"), estimate_tokens("abc λ"));
    }

    #[test]
    fn invalid_policy_is_rejected() {
        let mut policy = ContextPolicy::default();
        policy.max_items = policy.max_candidates + 1;
        assert!(matches!(
            policy.validate(),
            Err(ContextError::InvalidPolicy)
        ));
    }
}
