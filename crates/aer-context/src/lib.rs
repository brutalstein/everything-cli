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
use aer_repo::{FileKind, RepositoryIndex, SearchHit, SearchQuery};
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

        for semantic_id in &request.required_semantic_ids {
            let links = index.semantic_links(&snapshot_id, semantic_id)?;
            if links.is_empty() {
                return Err(ContextError::MandatoryCoverageUnavailable(
                    semantic_id.clone(),
                ));
            }
            let mut resolved = false;
            for (rank, link) in links.into_iter().enumerate() {
                let Some(resolved_candidate) =
                    resolve_path_candidate(index, &snapshot_id, &link.target_path)?
                else {
                    continue;
                };
                resolved = true;
                let path = resolved_candidate.path.clone();
                let candidate = candidates.entry(path).or_insert(resolved_candidate);
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

        for symbol in &request.required_symbols {
            let definitions = index.definitions(&snapshot_id, symbol)?;
            if definitions.is_empty() {
                return Err(ContextError::ExactDefinitionUnavailable(symbol.clone()));
            }
            if definitions.len() > self.policy.max_definitions_per_symbol {
                return Err(ContextError::ExactDefinitionAmbiguous {
                    symbol: symbol.clone(),
                    definitions: definitions.len(),
                });
            }
            for definition in definitions {
                let start = definition.start_line.max(1);
                let end = definition.end_line.max(start);
                let lines = end.saturating_sub(start).saturating_add(1);
                if lines > self.policy.max_required_definition_lines {
                    return Err(ContextError::ExactDefinitionTooLarge {
                        symbol: symbol.clone(),
                        lines,
                        maximum: self.policy.max_required_definition_lines,
                    });
                }
                let Some(resolved_candidate) =
                    resolve_path_candidate(index, &snapshot_id, &definition.path)?
                else {
                    return Err(ContextError::ExactDefinitionUnavailable(symbol.clone()));
                };
                let path = resolved_candidate.path.clone();
                let candidate = candidates.entry(path).or_insert(resolved_candidate);
                candidate.required_definitions.insert((start, end));
                candidate.required_symbols.insert(symbol.clone());
                candidate
                    .reasons
                    .insert(format!("exact definition: {symbol}"));
            }
        }

        for hint in &request.runtime_hints {
            if hint.score_milli == 0 {
                continue;
            }
            let Some(resolved_candidate) = resolve_path_candidate(index, &snapshot_id, &hint.path)?
            else {
                continue;
            };
            let path = resolved_candidate.path.clone();
            let candidate = candidates.entry(path).or_insert(resolved_candidate);
            candidate.runtime_rank =
                min_rank(candidate.runtime_rank, runtime_rank(hint.score_milli));
            candidate
                .reasons
                .insert(format!("runtime hint: {}", hint.reason));
        }

        if structural_retrieval_required(&demands, &candidates) {
            retrieval_trace
                .stages_invoked
                .push(RetrievalStage::Structural);
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

        let pack = self.select(
            workspace_root,
            index,
            &snapshot_id,
            request,
            &demands,
            retrieval_trace,
            ranked,
        )?;

        if index.verified_current_snapshot_id(workspace_root)? != snapshot_id {
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
        if index.verified_current_snapshot_id(workspace_root)? != pack.repo_snapshot {
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
        demands: &[EvidenceDemand],
        mut retrieval_trace: RetrievalTrace,
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
        for candidate in ranked
            .iter()
            .filter(|candidate| !candidate.required_definitions.is_empty())
        {
            mandatory_paths.insert(candidate.path.clone());
        }
        for candidate in &ranked {
            if demands.iter().any(|demand| {
                demand.verification_critical
                    && !matches!(demand.target, EvidenceDemandTarget::Objective)
                    && candidate_covers_demand(candidate, demand)
            }) {
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
        for candidate in materialized
            .iter()
            .filter(|candidate| !candidate.required_spans.is_empty())
        {
            let item = candidate.required_item(&self.policy)?;
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

        // Evidence sufficiency, not spare budget, drives selection. A candidate
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
                item.required_semantic_ids
                    .iter()
                    .any(|id| id == semantic_id)
            }) {
                return Err(ContextError::MandatoryCoverageUnavailable(
                    semantic_id.clone(),
                ));
            }
        }

        // Fail closed rather than ship a pack that silently omits a definition the
        // task named. Selection above reserves these first; this re-checks the
        // invariant against the produced items.
        for symbol in &request.required_symbols {
            let mut covered = false;
            for candidate in materialized
                .iter()
                .filter(|candidate| candidate.candidate.required_symbols.contains(symbol))
            {
                let Some(item) = selected.get(&candidate.candidate.path) else {
                    return Err(ContextError::ExactDefinitionUnavailable(symbol.clone()));
                };
                for (start, end) in &candidate.required_spans {
                    if !item
                        .segments
                        .iter()
                        .any(|segment| segment.start_line <= *start && segment.end_line >= *end)
                    {
                        return Err(ContextError::ExactDefinitionUnavailable(symbol.clone()));
                    }
                }
                covered = true;
            }
            if !covered {
                return Err(ContextError::ExactDefinitionUnavailable(symbol.clone()));
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
            evidence_demands: demands.to_vec(),
            retrieval_trace,
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
        let required_spans = merge_spans(&candidate.required_definitions, line_count);
        Ok(MaterializedCandidate {
            candidate: candidate.clone(),
            text,
            line_count,
            symbol_span,
            required_spans,
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
    required_symbols: BTreeSet<String>,
    required_definitions: BTreeSet<(u32, u32)>,
    reasons: BTreeSet<String>,
    lexical_rank: Option<usize>,
    semantic_rank: Option<usize>,
    structural_rank: Option<usize>,
    runtime_rank: Option<usize>,
    utility_micros: u64,
}

impl Candidate {
    fn from_source(path: String, content_hash: String) -> Self {
        Self {
            path,
            content_hash,
            anchor_line: None,
            matched_terms: BTreeSet::new(),
            matched_symbols: BTreeSet::new(),
            required_semantic_ids: BTreeSet::new(),
            required_symbols: BTreeSet::new(),
            required_definitions: BTreeSet::new(),
            reasons: BTreeSet::new(),
            lexical_rank: None,
            semantic_rank: None,
            structural_rank: None,
            runtime_rank: None,
            utility_micros: 0,
        }
    }

    fn from_hit(hit: &SearchHit) -> Self {
        let mut candidate = Self {
            path: hit.path.clone(),
            content_hash: hit.content_sha256.clone(),
            anchor_line: hit.anchor_line,
            matched_terms: BTreeSet::new(),
            matched_symbols: BTreeSet::new(),
            required_semantic_ids: BTreeSet::new(),
            required_symbols: BTreeSet::new(),
            required_definitions: BTreeSet::new(),
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
    required_spans: Vec<(u32, u32)>,
}

impl MaterializedCandidate {
    /// Exactly named definitions are materialized verbatim and never traded down
    /// to a narrower tier, so a task that names an identifier either receives the
    /// defining span or the compilation fails closed.
    fn required_item(&self, policy: &ContextPolicy) -> Result<ContextItem, ContextError> {
        let mut segments = Vec::with_capacity(self.required_spans.len());
        let mut body = String::new();
        for (start, end) in &self.required_spans {
            segments.push(source_segment(&self.text, *start, *end)?);
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(&format!(
                "{}#L{}-L{}\n{}",
                self.candidate.path,
                start,
                end,
                extract_lines(&self.text, *start, *end)?
            ));
        }
        self.finish(ContextTier::Expanded, segments, body, policy)
    }

    fn tier(&self, tier: ContextTier, policy: &ContextPolicy) -> Result<ContextItem, ContextError> {
        if !self.required_spans.is_empty() {
            return self.required_item(policy);
        }
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
        self.finish(tier, segments, body, policy)
    }

    fn finish(
        &self,
        tier: ContextTier,
        segments: Vec<SourceSegment>,
        body: String,
        policy: &ContextPolicy,
    ) -> Result<ContextItem, ContextError> {
        let token_cost = estimate_tokens(&body)
            .saturating_add(policy.per_item_token_overhead)
            .max(1);
        let source_ref = if segments.is_empty() {
            self.candidate.path.clone()
        } else {
            format!(
                "{}#{}",
                self.candidate.path,
                segments
                    .iter()
                    .map(|segment| format!("L{}-L{}", segment.start_line, segment.end_line))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let mut required_semantic_ids = self
            .candidate
            .required_semantic_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        required_semantic_ids.sort();
        Ok(ContextItem {
            id: context_item_id(&self.candidate.content_hash, &source_ref, tier, &segments),
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
    if request.required_symbols.len() > policy.max_required_symbols {
        return Err(ContextError::InvalidRequest(format!(
            "required symbol count exceeds {}",
            policy.max_required_symbols
        )));
    }
    if request
        .required_symbols
        .iter()
        .any(|symbol| symbol.trim().is_empty())
    {
        return Err(ContextError::InvalidRequest(
            "required symbols cannot be empty".to_owned(),
        ));
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

fn compile_evidence_demands(request: &ContextRequest) -> Vec<EvidenceDemand> {
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
        let wants_test_context = objective_lower.contains("test");
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
        if wants_test_context {
            demands.push(EvidenceDemand {
                demand_id: "objective:test-context".to_owned(),
                kind: EvidenceDemandKind::TestContext,
                target: EvidenceDemandTarget::Objective,
                minimum_tier: ContextTier::SourceSpan,
                required_provenance: EvidenceProvenance::IndexedSource,
                minimum_coverage: 1,
                expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,
                importance_milli: 850,
                verification_critical: false,
            });
        } else {
            demands.push(EvidenceDemand {
                demand_id: "objective:supporting-context".to_owned(),
                kind: EvidenceDemandKind::SupportingContext,
                target: EvidenceDemandTarget::Objective,
                minimum_tier: ContextTier::Structural,
                required_provenance: EvidenceProvenance::IndexedSource,
                minimum_coverage: 1,
                expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,
                importance_milli: 700,
                verification_critical: false,
            });
        }
    }
    if request.objective.to_ascii_lowercase().contains("test")
        && !demands
            .iter()
            .any(|demand| demand.kind == EvidenceDemandKind::TestContext)
    {
        demands.push(EvidenceDemand {
            demand_id: "objective:test-context".to_owned(),
            kind: EvidenceDemandKind::TestContext,
            target: EvidenceDemandTarget::Objective,
            minimum_tier: ContextTier::SourceSpan,
            required_provenance: EvidenceProvenance::IndexedSource,
            minimum_coverage: 1,
            expansion_policy: EvidenceExpansionPolicy::BoundedNeighborhood,
            importance_milli: 850,
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
        EvidenceDemandKind::ChangeImpact
            | EvidenceDemandKind::TestContext
            | EvidenceDemandKind::SupportingContext
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
            if demand.kind == EvidenceDemandKind::TestContext {
                is_test_source_path(&candidate.path)
            } else {
                candidate.lexical_rank.is_some() || candidate.structural_rank.is_some()
            }
        }
    }
}

fn is_test_source_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.contains("/test/")
        || path.ends_with("_test.rs")
        || path.ends_with("_test.py")
        || path.ends_with(".test.ts")
        || path.ends_with(".test.tsx")
        || path.ends_with(".test.js")
        || path.ends_with(".spec.ts")
        || path.ends_with(".spec.js")
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

fn resolve_path_candidate(
    index: &RepositoryIndex,
    snapshot_id: &str,
    path: &str,
) -> Result<Option<Candidate>, ContextError> {
    let Some(file) = index.file(snapshot_id, path)? else {
        return Ok(None);
    };
    if file.kind != FileKind::Text {
        return Ok(None);
    }
    let Some(content_hash) = file.content_sha256 else {
        return Ok(None);
    };
    Ok(Some(Candidate::from_source(file.path, content_hash)))
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
    if !candidate.required_definitions.is_empty() {
        value = value.saturating_add(RRF_SCALE.saturating_mul(2));
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
    segments: &[SourceSegment],
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"aer-context-item-v1\0");
    digest.update(content_hash.as_bytes());
    digest.update(b"\0");
    digest.update(source_ref.as_bytes());
    digest.update(b"\0");
    digest.update([tier.as_u8()]);
    for segment in segments {
        digest.update(b"\0");
        digest.update(segment.sha256.as_bytes());
    }
    format!("context-item:{}", hex(digest.finalize().as_ref()))
}

/// Merges the exact definition spans requested for one file into ordered,
/// non-overlapping ranges clamped to the file.
fn merge_spans(spans: &BTreeSet<(u32, u32)>, line_count: u32) -> Vec<(u32, u32)> {
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for (start, end) in spans {
        let start = (*start).clamp(1, line_count.max(1));
        let end = (*end).clamp(start, line_count.max(1));
        match merged.last_mut() {
            Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
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
