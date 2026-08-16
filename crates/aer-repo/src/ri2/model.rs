use crate::RepoError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CapabilityTier {
    Tier0Text,
    Tier1Syntax,
    Tier2Project,
    Tier3PreciseSemantic,
    Tier4DynamicEvidence,
}

impl CapabilityTier {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Tier0Text => 0,
            Self::Tier1Syntax => 1,
            Self::Tier2Project => 2,
            Self::Tier3PreciseSemantic => 3,
            Self::Tier4DynamicEvidence => 4,
        }
    }

    pub(crate) fn from_i64(value: i64) -> Result<Self, RepoError> {
        match value {
            0 => Ok(Self::Tier0Text),
            1 => Ok(Self::Tier1Syntax),
            2 => Ok(Self::Tier2Project),
            3 => Ok(Self::Tier3PreciseSemantic),
            4 => Ok(Self::Tier4DynamicEvidence),
            _ => Err(RepoError::Integrity(format!(
                "invalid RI2 capability tier: {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreshnessState {
    Current,
    PartiallyCurrent,
    Stale,
    Unavailable,
}

impl FreshnessState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::PartiallyCurrent => "partially_current",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, RepoError> {
        match value {
            "current" => Ok(Self::Current),
            "partially_current" => Ok(Self::PartiallyCurrent),
            "stale" => Ok(Self::Stale),
            "unavailable" => Ok(Self::Unavailable),
            _ => Err(RepoError::Integrity(format!(
                "invalid RI2 freshness state: {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceClass {
    Extracted,
    SemanticResolved,
    Observed,
    Inferred,
}

impl EvidenceClass {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Extracted => "extracted",
            Self::SemanticResolved => "semantic_resolved",
            Self::Observed => "observed",
            Self::Inferred => "inferred",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, RepoError> {
        match value {
            "extracted" => Ok(Self::Extracted),
            "semantic_resolved" => Ok(Self::SemanticResolved),
            "observed" => Ok(Self::Observed),
            "inferred" => Ok(Self::Inferred),
            _ => Err(RepoError::Integrity(format!(
                "invalid RI2 evidence class: {value}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageProfileView {
    pub language_id: String,
    pub aliases: Vec<String>,
    pub extensions: Vec<String>,
    pub filenames: Vec<String>,
    pub shebangs: Vec<String>,
    pub file_role: String,
    pub grammar_adapter: Option<String>,
    pub grammar_version: Option<String>,
    pub extraction_query_version: String,
    pub maximum_static_tier: CapabilityTier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageCapabilityReport {
    pub registry_version: String,
    pub text_files: usize,
    pub tier0_files: usize,
    pub tier1_files: usize,
    pub tier2_files: usize,
    pub tier3_files: usize,
    pub tier4_files: usize,
    pub fallback_files: usize,
    pub ambiguous_files: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewState {
    pub view_name: String,
    pub indexed_snapshot: String,
    pub producer_id: String,
    pub producer_version: String,
    pub freshness: FreshnessState,
    pub capability_tier: CapabilityTier,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphNodeKind {
    File,
    Symbol,
    SymbolCandidate,
    Package,
    BuildTarget,
    ExternalPackage,
    Test,
    RuntimeObservation,
    SemanticAnchor,
}

impl GraphNodeKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::SymbolCandidate => "symbol_candidate",
            Self::Package => "package",
            Self::BuildTarget => "build_target",
            Self::ExternalPackage => "external_package",
            Self::Test => "test",
            Self::RuntimeObservation => "runtime_observation",
            Self::SemanticAnchor => "semantic_anchor",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, RepoError> {
        match value {
            "file" => Ok(Self::File),
            "symbol" => Ok(Self::Symbol),
            "symbol_candidate" => Ok(Self::SymbolCandidate),
            "package" => Ok(Self::Package),
            "build_target" => Ok(Self::BuildTarget),
            "external_package" => Ok(Self::ExternalPackage),
            "test" => Ok(Self::Test),
            "runtime_observation" => Ok(Self::RuntimeObservation),
            "semantic_anchor" => Ok(Self::SemanticAnchor),
            _ => Err(RepoError::Integrity(format!(
                "invalid RI2 node kind: {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphEdgeKind {
    Defines,
    Imports,
    Calls,
    References,
    ResolvesTo,
    DependsOn,
    Builds,
    Tests,
    ChangedWith,
    RenamedFrom,
    ObservedIn,
    Supports,
    Implements,
    Inherits,
}

impl GraphEdgeKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Defines => "defines",
            Self::Imports => "imports",
            Self::Calls => "calls",
            Self::References => "references",
            Self::ResolvesTo => "resolves_to",
            Self::DependsOn => "depends_on",
            Self::Builds => "builds",
            Self::Tests => "tests",
            Self::ChangedWith => "changed_with",
            Self::RenamedFrom => "renamed_from",
            Self::ObservedIn => "observed_in",
            Self::Supports => "supports",
            Self::Implements => "implements",
            Self::Inherits => "inherits",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, RepoError> {
        match value {
            "defines" => Ok(Self::Defines),
            "imports" => Ok(Self::Imports),
            "calls" => Ok(Self::Calls),
            "references" => Ok(Self::References),
            "resolves_to" => Ok(Self::ResolvesTo),
            "depends_on" => Ok(Self::DependsOn),
            "builds" => Ok(Self::Builds),
            "tests" => Ok(Self::Tests),
            "changed_with" => Ok(Self::ChangedWith),
            "renamed_from" => Ok(Self::RenamedFrom),
            "observed_in" => Ok(Self::ObservedIn),
            "supports" => Ok(Self::Supports),
            "implements" => Ok(Self::Implements),
            "inherits" => Ok(Self::Inherits),
            _ => Err(RepoError::Integrity(format!(
                "invalid RI2 edge kind: {value}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphNode {
    pub node_id: String,
    pub kind: GraphNodeKind,
    pub label: String,
    pub path: Option<String>,
    pub source_line: Option<u32>,
    pub content_sha256: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdgeEvidence {
    pub evidence_class: EvidenceClass,
    pub confidence_milli: u16,
    pub producer_id: String,
    pub producer_version: String,
    pub repo_snapshot: String,
    pub source_path: Option<String>,
    pub source_line: Option<u32>,
    pub environment_fingerprint: Option<String>,
    pub valid_from_snapshot: String,
    pub valid_until_snapshot: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphEdge {
    pub edge_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub kind: GraphEdgeKind,
    pub evidence: EdgeEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphDirection {
    Outgoing,
    Incoming,
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraversalBudget {
    pub max_depth: u16,
    pub max_nodes: usize,
    pub max_edges: usize,
}

impl TraversalBudget {
    pub fn validate(self) -> Result<Self, RepoError> {
        if self.max_depth == 0 || self.max_nodes == 0 || self.max_edges == 0 {
            return Err(RepoError::InvalidPolicy);
        }
        Ok(self)
    }
}

impl Default for TraversalBudget {
    fn default() -> Self {
        Self {
            max_depth: 2,
            max_nodes: 128,
            max_edges: 512,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphQueryResult {
    pub snapshot_id: String,
    pub root_node_ids: Vec<String>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildPackage {
    pub package_id: String,
    pub manager: String,
    pub name: String,
    pub version: String,
    pub manifest_path: String,
    pub workspace_member: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildTarget {
    pub target_id: String,
    pub package_id: String,
    pub name: String,
    pub kind: String,
    pub source_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectDependency {
    pub source_package_id: String,
    pub target_name: String,
    pub target_package_id: Option<String>,
    pub dependency_kind: String,
    pub manifest_path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreciseRelation {
    Definition,
    Reference,
    Call,
    Implementation,
    Inheritance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreciseSemanticEdge {
    pub source_path: String,
    pub source_line: Option<u32>,
    pub source_symbol_id: Option<String>,
    pub relation: PreciseRelation,
    pub target_symbol: String,
    pub target_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreciseSemanticBatch {
    pub snapshot_id: String,
    pub producer_id: String,
    pub producer_version: String,
    pub environment_fingerprint: String,
    pub edges: Vec<PreciseSemanticEdge>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolContinuity {
    pub logical_symbol_id: String,
    pub from_snapshot: String,
    pub from_symbol_id: String,
    pub to_snapshot: String,
    pub to_symbol_id: String,
    pub evidence_class: EvidenceClass,
    pub confidence_milli: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryChangeSet {
    pub from_snapshot: String,
    pub to_snapshot: String,
    pub added_paths: Vec<String>,
    pub changed_paths: Vec<String>,
    pub deleted_paths: Vec<String>,
    pub invalidated_entity_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ri2RetrievalHit {
    pub path: String,
    pub why_relevant: Vec<String>,
    pub capability_tier: CapabilityTier,
    pub provenance: Vec<EvidenceClass>,
    pub freshness: FreshnessState,
    pub confidence_milli: u16,
}
