use std::{error::Error, fmt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPolicy {
    pub version: String,
    pub max_candidates: usize,
    pub max_items: usize,
    pub max_source_bytes: u64,
    pub max_span_lines: u32,
    pub max_tier3_lines: u32,
    pub max_semantic_ids: usize,
    pub max_runtime_hints: usize,
    pub max_impact_seeds: usize,
    pub omitted_high_rank_limit: usize,
    pub rrf_k: u32,
    pub lexical_weight: u32,
    pub semantic_weight: u32,
    pub structural_weight: u32,
    pub runtime_weight: u32,
    pub base_pack_token_overhead: u32,
    pub per_item_token_overhead: u32,
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self {
            version: "context-policy-v1".to_owned(),
            max_candidates: 128,
            max_items: 24,
            max_source_bytes: 4 * 1024 * 1024,
            max_span_lines: 80,
            max_tier3_lines: 240,
            max_semantic_ids: 32,
            max_runtime_hints: 64,
            max_impact_seeds: 8,
            omitted_high_rank_limit: 16,
            rrf_k: 60,
            lexical_weight: 1000,
            semantic_weight: 1400,
            structural_weight: 900,
            runtime_weight: 1200,
            base_pack_token_overhead: 32,
            per_item_token_overhead: 16,
        }
    }
}

impl ContextPolicy {
    pub fn validate(&self) -> Result<(), ContextError> {
        if self.version.trim().is_empty()
            || self.max_candidates == 0
            || self.max_items == 0
            || self.max_items > self.max_candidates
            || self.max_source_bytes == 0
            || self.max_span_lines == 0
            || self.max_tier3_lines < self.max_span_lines
            || self.max_semantic_ids == 0
            || self.max_runtime_hints == 0
            || self.max_impact_seeds == 0
            || self.omitted_high_rank_limit == 0
            || self.rrf_k == 0
            || self.lexical_weight == 0
            || self.semantic_weight == 0
            || self.structural_weight == 0
            || self.runtime_weight == 0
        {
            return Err(ContextError::InvalidPolicy);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextRequest {
    pub task_id: String,
    pub objective: String,
    pub engineering_ir_version: u64,
    pub input_token_budget: u32,
    pub required_semantic_ids: Vec<String>,
    pub runtime_hints: Vec<RuntimeHint>,
}

impl ContextRequest {
    #[must_use]
    pub fn new(
        task_id: impl Into<String>,
        objective: impl Into<String>,
        engineering_ir_version: u64,
        input_token_budget: u32,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            objective: objective.into(),
            engineering_ir_version,
            input_token_budget,
            required_semantic_ids: Vec::new(),
            runtime_hints: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeHint {
    pub path: String,
    pub score_milli: u16,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContextTier {
    Identifier = 0,
    Structural = 1,
    SourceSpan = 2,
    Expanded = 3,
}

impl ContextTier {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSegment {
    pub start_line: u32,
    pub end_line: u32,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextItem {
    pub id: String,
    pub source_type: String,
    pub source_ref: String,
    pub content_hash: String,
    pub token_cost: u32,
    pub tier: ContextTier,
    pub selected_reason: String,
    pub path: String,
    pub utility_micros: u64,
    pub required_semantic_ids: Vec<String>,
    pub segments: Vec<SourceSegment>,
    pub rendered_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPack {
    pub schema_version: u32,
    pub pack_id: String,
    pub task_id: String,
    pub engineering_ir_version: u64,
    pub repo_snapshot: String,
    pub policy_version: String,
    pub input_token_budget: u32,
    pub items: Vec<ContextItem>,
    pub omitted_high_rank_items: Vec<String>,
    pub source_hashes: Vec<String>,
}

impl ContextPack {
    #[must_use]
    pub fn total_token_cost(&self) -> u32 {
        self.items
            .iter()
            .fold(0_u32, |total, item| total.saturating_add(item.token_cost))
    }

    #[must_use]
    pub fn selected_paths(&self) -> Vec<&str> {
        self.items.iter().map(|item| item.path.as_str()).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextBenchMetrics {
    pub relevant_total: usize,
    pub relevant_selected: usize,
    pub recall_milli: u16,
    pub selected_tokens: u64,
    pub naive_tokens: u64,
    pub selected_yield_micros: u64,
    pub naive_yield_micros: u64,
}

#[derive(Debug)]
pub enum ContextError {
    InvalidPolicy,
    InvalidRequest(String),
    Repository(aer_repo::RepoError),
    Io(std::io::Error),
    SourceTooLarge { path: String, bytes: u64 },
    SourceHashMismatch { path: String },
    SourceNotRetrievable(String),
    MandatoryCoverageUnavailable(String),
    BudgetTooSmall { required: u32, available: u32 },
    ContractRegistry(String),
    ContractValidation(Vec<String>),
    Fidelity(String),
    Arithmetic(String),
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy => write!(f, "context policy is invalid"),
            Self::InvalidRequest(message) => write!(f, "context request is invalid: {message}"),
            Self::Repository(error) => write!(f, "repository context retrieval failed: {error}"),
            Self::Io(error) => write!(f, "context source I/O failed: {error}"),
            Self::SourceTooLarge { path, bytes } => {
                write!(
                    f,
                    "context source exceeds configured bound: {path} ({bytes} bytes)"
                )
            }
            Self::SourceHashMismatch { path } => {
                write!(
                    f,
                    "context source changed or does not match indexed hash: {path}"
                )
            }
            Self::SourceNotRetrievable(path) => {
                write!(
                    f,
                    "repository source could not be resolved from the exact index: {path}"
                )
            }
            Self::MandatoryCoverageUnavailable(id) => {
                write!(
                    f,
                    "mandatory semantic context has no resolvable source: {id}"
                )
            }
            Self::BudgetTooSmall {
                required,
                available,
            } => write!(
                f,
                "context token budget is too small: requires at least {required}, available {available}"
            ),
            Self::ContractRegistry(message) => {
                write!(f, "Context Pack contract registry failed: {message}")
            }
            Self::ContractValidation(issues) => {
                write!(f, "Context Pack contract validation failed")?;
                for issue in issues {
                    write!(f, "; {issue}")?;
                }
                Ok(())
            }
            Self::Fidelity(message) => write!(f, "Context Pack fidelity failed: {message}"),
            Self::Arithmetic(message) => write!(f, "context accounting failed: {message}"),
        }
    }
}

impl Error for ContextError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Repository(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<aer_repo::RepoError> for ContextError {
    fn from(value: aer_repo::RepoError) -> Self {
        Self::Repository(value)
    }
}

impl From<std::io::Error> for ContextError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
