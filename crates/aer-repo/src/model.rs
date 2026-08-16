use std::{error::Error, fmt, path::PathBuf, time::Duration};

#[derive(Clone, Debug)]
pub struct IndexPolicy {
    pub max_files: usize,
    pub max_text_file_bytes: u64,
    pub max_total_text_bytes: u64,
    pub max_git_commits: usize,
    pub max_cochange_files_per_commit: usize,
    pub max_terms_per_file: usize,
    pub max_links_per_file: usize,
    pub max_results: usize,
    pub max_query_bytes: usize,
    pub retained_snapshots: usize,
    pub git_timeout: Duration,
    pub max_git_output_bytes: usize,
}

impl Default for IndexPolicy {
    fn default() -> Self {
        Self {
            max_files: 200_000,
            max_text_file_bytes: 4 * 1024 * 1024,
            max_total_text_bytes: 512 * 1024 * 1024,
            max_git_commits: 128,
            max_cochange_files_per_commit: 128,
            max_terms_per_file: 100_000,
            max_links_per_file: 20_000,
            max_results: 100,
            max_query_bytes: 16 * 1024,
            retained_snapshots: 4,
            git_timeout: Duration::from_secs(30),
            max_git_output_bytes: 64 * 1024 * 1024,
        }
    }
}

impl IndexPolicy {
    pub fn validate(&self) -> Result<(), RepoError> {
        if self.max_files == 0
            || self.max_text_file_bytes == 0
            || self.max_total_text_bytes == 0
            || self.max_git_commits == 0
            || self.max_cochange_files_per_commit == 0
            || self.max_terms_per_file == 0
            || self.max_links_per_file == 0
            || self.max_results == 0
            || self.max_query_bytes == 0
            || self.retained_snapshots == 0
            || self.git_timeout.is_zero()
            || self.max_git_output_bytes == 0
        {
            return Err(RepoError::InvalidPolicy);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoSnapshotIdentity {
    pub snapshot_id: String,
    pub repo_id: String,
    pub repo_root: PathBuf,
    pub head_commit: String,
    pub dirty_tracked_diff_sha256: String,
    pub untracked_content_sha256: String,
    pub submodule_state_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileKind {
    Text,
    Binary,
    Oversized,
    Symlink,
}

impl FileKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Binary => "binary",
            Self::Oversized => "oversized",
            Self::Symlink => "symlink",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LanguageKind {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Json,
    Toml,
    Markdown,
    Shell,
    Yaml,
    Other,
}

impl LanguageKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Markdown => "markdown",
            Self::Shell => "shell",
            Self::Yaml => "yaml",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedFile {
    pub path: String,
    pub content_sha256: Option<String>,
    pub byte_len: u64,
    pub line_count: u32,
    pub language: LanguageKind,
    pub kind: FileKind,
    pub parser_key: Option<String>,
    pub is_test: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Module,
    TypeAlias,
    Constant,
    Static,
    Macro,
    Variable,
    Test,
    Other,
}

impl SymbolKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Module => "module",
            Self::TypeAlias => "type_alias",
            Self::Constant => "constant",
            Self::Static => "static",
            Self::Macro => "macro",
            Self::Variable => "variable",
            Self::Test => "test",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolRecord {
    pub symbol_id: String,
    pub path: String,
    pub name: String,
    pub kind: SymbolKind,
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub signature: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgeKind {
    Imports,
    Calls,
    References,
}

impl EdgeKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Imports => "imports",
            Self::Calls => "calls",
            Self::References => "references",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyEdge {
    pub source_path: String,
    pub source_symbol_id: Option<String>,
    pub kind: EdgeKind,
    pub target_name: String,
    pub target_symbol_id: Option<String>,
    pub line: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestAssociation {
    pub test_path: String,
    pub target_path: String,
    pub target_symbol_id: Option<String>,
    pub target_symbol_name: Option<String>,
    pub confidence_milli: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitCommitView {
    pub commit: String,
    pub unix_time: i64,
    pub changed_paths: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoChangeRecord {
    pub path_a: String,
    pub path_b: String,
    pub count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticAnchor {
    pub kind: String,
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticLink {
    pub semantic_kind: String,
    pub semantic_id: String,
    pub target_path: String,
    pub target_symbol_id: Option<String>,
    pub score_micros: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeObservation {
    pub observation_id: String,
    pub path: String,
    pub line: Option<u32>,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeLink {
    pub observation_id: String,
    pub path: String,
    pub line: Option<u32>,
    pub summary: String,
    pub content_sha256: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery {
    pub text: String,
    pub limit: usize,
    pub min_score_micros: u64,
}

impl SearchQuery {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            limit: 12,
            min_score_micros: 100_000,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub path: String,
    pub content_sha256: String,
    pub language: LanguageKind,
    pub score_micros: u64,
    pub anchor_line: Option<u32>,
    pub matched_terms: Vec<String>,
    pub matched_symbols: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AbstentionReason {
    EmptyQuery,
    NoIndexedTerms,
    BelowConfidenceThreshold,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub snapshot_id: String,
    pub hits: Vec<SearchHit>,
    pub abstained: bool,
    pub abstention_reason: Option<AbstentionReason>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexBuildReport {
    pub snapshot: RepoSnapshotIdentity,
    pub already_current: bool,
    pub files_seen: usize,
    pub text_files: usize,
    pub binary_files: usize,
    pub oversized_files: usize,
    pub symlinks: usize,
    pub parsed_artifacts: usize,
    pub reused_artifacts: usize,
    pub symbols: usize,
    pub dependency_edges: usize,
    pub test_associations: usize,
    pub git_commits: usize,
    pub cochange_pairs: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImpactCandidate {
    pub path: String,
    pub reason: String,
    pub score_milli: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalCase {
    pub query: String,
    pub relevant_paths: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetrievalMetrics {
    pub cases: usize,
    pub relevant_total: usize,
    pub relevant_found: usize,
    pub recall_milli: u16,
}

#[derive(Debug)]
pub enum RepoError {
    InvalidPolicy,
    Io(std::io::Error),
    Workspace(aer_workspace::WorkspaceError),
    Execution(aer_exec::ExecutionError),
    Sqlite(rusqlite::Error),
    Git(String),
    OutputTooLarge { operation: &'static str, bytes: u64 },
    FileLimitExceeded(usize),
    TextBudgetExceeded(u64),
    NonUtf8Path,
    InvalidRelativePath(String),
    QueryTooLarge(usize),
    ResultLimitExceeded(usize),
    TreeSitter(String),
    WorkspaceChangedDuringIndex,
    UnknownSnapshot(String),
    StaleIndex { indexed: String, current: String },
    UnsupportedIndexVersion(i64),
    Integrity(String),
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy => write!(f, "repository index policy is invalid"),
            Self::Io(error) => write!(f, "repository I/O failed: {error}"),
            Self::Workspace(error) => write!(f, "workspace inspection failed: {error}"),
            Self::Execution(error) => write!(f, "repository command execution failed: {error}"),
            Self::Sqlite(error) => write!(f, "repository index SQLite failed: {error}"),
            Self::Git(error) => write!(f, "git repository view failed: {error}"),
            Self::OutputTooLarge { operation, bytes } => {
                write!(f, "{operation} output exceeded its bounded capture ({bytes} bytes)")
            }
            Self::FileLimitExceeded(count) => {
                write!(f, "repository file count exceeds configured limit: {count}")
            }
            Self::TextBudgetExceeded(bytes) => {
                write!(f, "repository text indexing budget exceeded at {bytes} bytes")
            }
            Self::NonUtf8Path => write!(f, "repository contains a non-UTF-8 path"),
            Self::InvalidRelativePath(path) => write!(f, "invalid repository-relative path: {path}"),
            Self::QueryTooLarge(bytes) => write!(f, "repository query exceeds limit: {bytes} bytes"),
            Self::ResultLimitExceeded(limit) => write!(f, "repository result limit exceeds policy: {limit}"),
            Self::TreeSitter(error) => write!(f, "Tree-sitter adapter failed: {error}"),
            Self::WorkspaceChangedDuringIndex => {
                write!(f, "workspace changed while repository index was being built")
            }
            Self::UnknownSnapshot(snapshot) => write!(f, "repository snapshot is not indexed: {snapshot}"),
            Self::StaleIndex { indexed, current } => write!(
                f,
                "repository index is stale: indexed snapshot {indexed}, current workspace {current}"
            ),
            Self::UnsupportedIndexVersion(version) => {
                write!(f, "unsupported derived repository-index schema version: {version}")
            }
            Self::Integrity(message) => write!(f, "repository index integrity failure: {message}"),
        }
    }
}

impl Error for RepoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Workspace(error) => Some(error),
            Self::Execution(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RepoError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<aer_workspace::WorkspaceError> for RepoError {
    fn from(value: aer_workspace::WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<aer_exec::ExecutionError> for RepoError {
    fn from(value: aer_exec::ExecutionError) -> Self {
        Self::Execution(value)
    }
}

impl From<rusqlite::Error> for RepoError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}
