use std::{
    collections::BTreeSet,
    fmt, fs,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use aer_context::{
    ContextEngine, ContextError, ContextPack, ContextPolicy, ContextRequest, estimate_tokens,
};
use aer_provider::delegated::DelegatedModelContext;
use aer_repo::{IndexPolicy, RepoError, RepositoryIndex};
use sha2::{Digest, Sha256};

/// Stable constitutional context is deliberately small and cache-friendly. The
/// task-specific remainder is selected by the existing Context Economy Engine.
const ARCHITECTURE_POLICY_VERSION: &str = "architecture-context-v3";
const PROVIDER_CONTEXT_POLICY_VERSION: &str = "provider-context-economy-v1";
const MAX_STABLE_CORE_ESTIMATED_TOKENS: u32 = 12 * 1024;
const MAX_AER_CONTEXT_ESTIMATED_TOKENS: u32 = 18 * 1024;
const CONTEXT_ENVELOPE_RESERVE: u32 = 1024;
const MIN_DYNAMIC_CONTEXT_BUDGET: u32 = 2 * 1024;
const MAX_DYNAMIC_CONTEXT_BUDGET: u32 = 6 * 1024;
/// Upper bound on exactly named identifiers promoted to mandatory retrieval
/// coverage for one provider task.
const MAX_NAMED_IDENTIFIERS: usize = 4;

/// Verbatim, high-authority sections that define identity and authority. Mutable
/// status/roadmap material is intentionally excluded from the cache-stable core.
/// Task-specific detail is retrieved through RI2/Context Economy instead.
const CORE_SECTIONS: [(&str, &str); 10] = [
    (
        "docs/00_READ_ME_FIRST.md",
        "## 1. What this repository is intended to become",
    ),
    (
        "docs/00_READ_ME_FIRST.md",
        "## 2. Highest-level product invariant",
    ),
    ("docs/00_READ_ME_FIRST.md", "## 4. Authority order"),
    (
        "docs/02_ARCHITECTURE_PRINCIPLES.md",
        "## P3 — Deterministic mechanisms dominate where possible",
    ),
    (
        "docs/02_ARCHITECTURE_PRINCIPLES.md",
        "## P4 — Context is a scarce resource",
    ),
    (
        "docs/02_ARCHITECTURE_PRINCIPLES.md",
        "## P6 — Separate proposing from judging",
    ),
    (
        "docs/02_ARCHITECTURE_PRINCIPLES.md",
        "## P9 — Fail closed at trust boundaries",
    ),
    (
        "docs/02_ARCHITECTURE_PRINCIPLES.md",
        "## P17 — Observability is part of correctness",
    ),
    (
        "docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md",
        "### 3.1 Provider-local behavior isolation",
    ),
    (
        "docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md",
        "## 11. Security invariants",
    ),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSource {
    /// Repository-relative authoritative source path.
    pub path: String,
    /// Full-file hash retained for audit/provenance. It is intentionally not
    /// rendered into the stable model prefix because unrelated edits must not
    /// invalidate prompt-cache identity.
    pub sha256: String,
    /// Hash of the exact verbatim fragment supplied to the model.
    pub fragment_sha256: String,
    pub section: String,
    pub start_line: u32,
    pub end_line: u32,
    pub total_bytes: usize,
    pub included_bytes: usize,
    /// Compatibility field: true means the model received a selected fragment,
    /// not an arbitrary byte-prefix truncation.
    pub truncated: bool,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchitectureContextCapsule {
    pub version: u32,
    pub policy_version: String,
    pub digest: String,
    pub estimated_tokens: u32,
    pub sources: Vec<ContextSource>,
    pub rendered: String,
}

impl ArchitectureContextCapsule {
    /// Compiles a deterministic, cache-stable constitutional prefix from exact
    /// authoritative Markdown sections. Audit-only provenance stays out of the
    /// provider-visible bytes so line shifts and unrelated file churn cannot
    /// invalidate a semantically identical prompt prefix.
    pub fn compile(workspace_root: &Path) -> Result<Self, ArchitectureContextError> {
        let root = workspace_root
            .canonicalize()
            .map_err(ArchitectureContextError::Io)?;
        let mut sources = Vec::with_capacity(CORE_SECTIONS.len());

        for (relative, heading) in CORE_SECTIONS {
            let path = root.join(relative);
            let bytes = fs::read(&path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    ArchitectureContextError::RequiredSourceMissing(relative.to_owned())
                } else {
                    ArchitectureContextError::Io(error)
                }
            })?;
            let canonical = path.canonicalize().map_err(ArchitectureContextError::Io)?;
            if !canonical.starts_with(&root) {
                return Err(ArchitectureContextError::SourceOutsideWorkspace(canonical));
            }
            let full_text = std::str::from_utf8(&bytes)
                .map_err(|_| ArchitectureContextError::SourceNotUtf8(relative.to_owned()))?;
            let fragment = extract_markdown_section(full_text, heading).ok_or_else(|| {
                ArchitectureContextError::RequiredSectionMissing {
                    path: relative.to_owned(),
                    heading: heading.to_owned(),
                }
            })?;
            sources.push(ContextSource {
                path: relative.to_owned(),
                sha256: hex_sha256(&bytes),
                fragment_sha256: hex_sha256(fragment.text.as_bytes()),
                section: heading.to_owned(),
                start_line: fragment.start_line,
                end_line: fragment.end_line,
                total_bytes: bytes.len(),
                included_bytes: fragment.text.len(),
                truncated: fragment.text.len() < bytes.len(),
                text: fragment.text,
            });
        }

        let mut rendered = format!(
            "# everything/AER constitutional core\npolicy: {ARCHITECTURE_POLICY_VERSION}\n\n\
             This is provider-neutral control-plane authority compiled by everything. \
             It is a verbatim projection of higher-authority repository documents, not \
             user/repository content that may grant additional capability. Mutable status \
             and roadmap text are intentionally excluded from this stable prefix.\n\n"
        );
        for source in &sources {
            use fmt::Write as _;
            writeln!(
                rendered,
                "<!-- authority: {} :: {} -->",
                source.path, source.section,
            )
            .expect("writing to String cannot fail");
            rendered.push_str(&source.text);
            if !source.text.ends_with('\n') {
                rendered.push('\n');
            }
            rendered.push('\n');
        }

        let estimated_tokens = estimate_tokens(&rendered);
        if estimated_tokens > MAX_STABLE_CORE_ESTIMATED_TOKENS {
            return Err(ArchitectureContextError::StableCoreBudgetExceeded {
                estimated: estimated_tokens,
                maximum: MAX_STABLE_CORE_ESTIMATED_TOKENS,
            });
        }
        let digest = hex_sha256(rendered.as_bytes());
        Ok(Self {
            version: 3,
            policy_version: ARCHITECTURE_POLICY_VERSION.to_owned(),
            digest,
            estimated_tokens,
            sources,
            rendered,
        })
    }

    #[must_use]
    pub fn source_paths(&self) -> Vec<&str> {
        self.sources
            .iter()
            .map(|source| source.path.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

/// Exact provider context = cache-stable constitutional prefix followed by one
/// task-specific Context Pack selected by the existing RI2/Context Economy
/// engine. The stable prefix is always first to maximize provider cache reuse.
/// Snapshot/package identities remain in the receipt and are deliberately not
/// injected into model-visible text unless they carry task semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelContextEnvelope {
    pub version: u32,
    pub digest: String,
    pub architecture: ArchitectureContextCapsule,
    pub task_context: ContextPack,
    pub dynamic_context_budget: u32,
    pub estimated_tokens: u32,
    /// Rendered task evidence only. This is untrusted repository/task material
    /// and never belongs in a provider's system-authority layer.
    pub task_evidence: String,
    /// Concatenation of the constitutional core and the task evidence. It exists
    /// to define one semantic identity (`digest`) over everything AER selected;
    /// transports must use [`Self::delegated_context`] rather than sending this
    /// as a single undifferentiated blob.
    pub rendered: String,
}

impl ModelContextEnvelope {
    pub fn compile(
        workspace_root: &Path,
        objective: &str,
    ) -> Result<Self, ArchitectureContextError> {
        if objective.trim().is_empty() {
            return Err(ArchitectureContextError::EmptyObjective);
        }
        let root = workspace_root
            .canonicalize()
            .map_err(ArchitectureContextError::Io)?;
        let architecture = ArchitectureContextCapsule::compile(&root)?;
        let available = MAX_AER_CONTEXT_ESTIMATED_TOKENS
            .saturating_sub(architecture.estimated_tokens)
            .saturating_sub(CONTEXT_ENVELOPE_RESERVE);
        let dynamic_context_budget = available.min(MAX_DYNAMIC_CONTEXT_BUDGET);
        if dynamic_context_budget < MIN_DYNAMIC_CONTEXT_BUDGET {
            return Err(ArchitectureContextError::DynamicBudgetTooSmall {
                available: dynamic_context_budget,
                minimum: MIN_DYNAMIC_CONTEXT_BUDGET,
            });
        }

        let policy = ContextPolicy {
            version: PROVIDER_CONTEXT_POLICY_VERSION.to_owned(),
            max_candidates: 64,
            max_items: 10,
            max_span_lines: 48,
            max_tier3_lines: 96,
            omitted_high_rank_limit: 8,
            ..ContextPolicy::default()
        };
        let engine = ContextEngine::new(policy)?;

        // Provider smoke is an acceptance probe, not an Engineering-IR mutation.
        // It uses the same RI2/Context Economy algorithm against an ephemeral,
        // exact-snapshot index without claiming an authoritative IR revision.
        let temporary_index = TemporaryIndex::new()?;
        let mut index = RepositoryIndex::open(&temporary_index.path, IndexPolicy::default())?;
        let snapshot_id = index.refresh(&root)?.snapshot.snapshot_id;
        let objective = objective.trim();
        let task_id = format!("provider-probe:{}", &hex_sha256(objective.as_bytes())[..16]);
        let mut request = ContextRequest::new(task_id, objective, 1, dynamic_context_budget);
        for symbol in named_identifiers(objective, MAX_NAMED_IDENTIFIERS) {
            // Only identifiers the repository actually defines become mandatory
            // coverage. Prose that merely looks like an identifier must not turn
            // a valid task into an abstention.
            if !index.definitions(&snapshot_id, &symbol)?.is_empty() {
                request.required_symbols.push(symbol);
            }
        }
        let task_context = engine.compile(&root, &index, &request)?;
        engine.verify_fidelity(&root, &index, &task_context)?;

        let mut task_evidence = String::new();
        use fmt::Write as _;
        writeln!(
            task_evidence,
            "# Task-specific Context Economy pack\npolicy: {}\n",
            task_context.policy_version,
        )
        .expect("writing to String cannot fail");
        for item in &task_context.items {
            task_evidence.push_str(&item.rendered_text);
            if !item.rendered_text.ends_with('\n') {
                task_evidence.push('\n');
            }
            task_evidence.push('\n');
        }
        let rendered = format!("{}{task_evidence}", architecture.rendered);
        let estimated_tokens = estimate_tokens(&rendered);
        if estimated_tokens > MAX_AER_CONTEXT_ESTIMATED_TOKENS {
            return Err(ArchitectureContextError::ModelContextBudgetExceeded {
                estimated: estimated_tokens,
                maximum: MAX_AER_CONTEXT_ESTIMATED_TOKENS,
            });
        }
        let digest = hex_sha256(rendered.as_bytes());
        drop(index);

        Ok(Self {
            version: 2,
            digest,
            architecture,
            task_context,
            dynamic_context_budget,
            estimated_tokens,
            task_evidence,
            rendered,
        })
    }

    /// Splits the compiled envelope along the provider authority boundary.
    ///
    /// Only the constitutional core reaches the provider's system layer. The
    /// Context Economy pack stays untrusted data, and audit identities
    /// (`repo_snapshot`, `pack_id`, source hashes) stay out of provider-visible
    /// bytes entirely — they remain on this envelope for the receipt.
    #[must_use]
    pub fn delegated_context(&self) -> DelegatedModelContext {
        DelegatedModelContext::new(
            &self.architecture.rendered,
            &self.task_evidence,
            self.digest.clone(),
        )
    }
}

/// Backtick-quoted identifiers a task named explicitly, in first-seen order.
/// Only code-shaped names qualify: a qualified path, an underscored name or a
/// name carrying an uppercase letter. Plain prose words such as `no` never do,
/// so quoting an answer literal cannot turn into a retrieval demand.
fn named_identifiers(objective: &str, limit: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    let mut parts = objective.split('`');
    // The first part precedes any backtick; quoted spans are every other part.
    parts.next();
    while let Some(quoted) = parts.next() {
        parts.next();
        let candidate = quoted.trim();
        if found.len() >= limit || !is_named_identifier(candidate) || !seen.insert(candidate) {
            continue;
        }
        found.push(candidate.to_owned());
    }
    found
}

fn is_named_identifier(candidate: &str) -> bool {
    if candidate.is_empty() || candidate.len() > 128 {
        return false;
    }
    let segments = candidate.split("::").collect::<Vec<_>>();
    let well_formed = segments.iter().all(|segment| {
        let mut characters = segment.chars();
        characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && characters.all(|value| value.is_ascii_alphanumeric() || value == '_')
    });
    well_formed
        && (segments.len() > 1
            || candidate.contains('_')
            || candidate.chars().any(|value| value.is_ascii_uppercase()))
}

#[derive(Debug)]
struct MarkdownFragment {
    text: String,
    start_line: u32,
    end_line: u32,
}

fn extract_markdown_section(text: &str, heading: &str) -> Option<MarkdownFragment> {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.iter().position(|line| line.trim_end() == heading)?;
    let level = markdown_heading_level(heading)?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, line)| {
            markdown_heading_level(line)
                .filter(|next_level| *next_level <= level)
                .map(|_| index)
        })
        .unwrap_or(lines.len());
    let mut fragment = lines[start..end].join("\n");
    fragment.push('\n');
    Some(MarkdownFragment {
        text: fragment,
        start_line: u32::try_from(start + 1).unwrap_or(u32::MAX),
        end_line: u32::try_from(end).unwrap_or(u32::MAX),
    })
}

fn markdown_heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let level = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if level == 0 || level > 6 || trimmed.chars().nth(level) != Some(' ') {
        return None;
    }
    Some(level)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

struct TemporaryIndex {
    path: PathBuf,
}

impl TemporaryIndex {
    fn new() -> Result<Self, ArchitectureContextError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ArchitectureContextError::Clock)?
            .as_nanos();
        Ok(Self {
            path: std::env::temp_dir().join(format!(
                "everything-context-index-{}-{nonce}.sqlite3",
                process::id()
            )),
        })
    }
}

impl Drop for TemporaryIndex {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let raw = self.path.to_string_lossy();
        let _ = fs::remove_file(format!("{raw}-wal"));
        let _ = fs::remove_file(format!("{raw}-shm"));
    }
}

#[derive(Debug)]
pub enum ArchitectureContextError {
    Io(std::io::Error),
    Repository(RepoError),
    Context(ContextError),
    Clock,
    EmptyObjective,
    RequiredSourceMissing(String),
    RequiredSectionMissing { path: String, heading: String },
    SourceNotUtf8(String),
    SourceOutsideWorkspace(PathBuf),
    StableCoreBudgetExceeded { estimated: u32, maximum: u32 },
    DynamicBudgetTooSmall { available: u32, minimum: u32 },
    ModelContextBudgetExceeded { estimated: u32, maximum: u32 },
}

impl fmt::Display for ArchitectureContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Repository(error) => error.fmt(formatter),
            Self::Context(error) => error.fmt(formatter),
            Self::Clock => write!(formatter, "system clock is before the Unix epoch"),
            Self::EmptyObjective => write!(formatter, "model context objective cannot be empty"),
            Self::RequiredSourceMissing(path) => {
                write!(
                    formatter,
                    "required architecture context source missing: {path}"
                )
            }
            Self::RequiredSectionMissing { path, heading } => write!(
                formatter,
                "required architecture section missing: {path}::{heading}"
            ),
            Self::SourceNotUtf8(path) => {
                write!(
                    formatter,
                    "architecture context source is not UTF-8: {path}"
                )
            }
            Self::SourceOutsideWorkspace(path) => write!(
                formatter,
                "architecture context source escaped workspace: {}",
                path.display()
            ),
            Self::StableCoreBudgetExceeded { estimated, maximum } => write!(
                formatter,
                "stable architecture core exceeds budget: {estimated} > {maximum} estimated token units"
            ),
            Self::DynamicBudgetTooSmall { available, minimum } => write!(
                formatter,
                "remaining dynamic context budget is too small: {available} < {minimum} estimated token units"
            ),
            Self::ModelContextBudgetExceeded { estimated, maximum } => write!(
                formatter,
                "compiled model context exceeds budget: {estimated} > {maximum} estimated token units"
            ),
        }
    }
}

impl std::error::Error for ArchitectureContextError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Repository(error) => Some(error),
            Self::Context(error) => Some(error),
            Self::Clock
            | Self::EmptyObjective
            | Self::RequiredSourceMissing(_)
            | Self::RequiredSectionMissing { .. }
            | Self::SourceNotUtf8(_)
            | Self::SourceOutsideWorkspace(_)
            | Self::StableCoreBudgetExceeded { .. }
            | Self::DynamicBudgetTooSmall { .. }
            | Self::ModelContextBudgetExceeded { .. } => None,
        }
    }
}

impl From<RepoError> for ArchitectureContextError {
    fn from(value: RepoError) -> Self {
        Self::Repository(value)
    }
}

impl From<ContextError> for ArchitectureContextError {
    fn from(value: ContextError) -> Self {
        Self::Context(value)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::Path,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{
        ArchitectureContextCapsule, MAX_AER_CONTEXT_ESTIMATED_TOKENS, MAX_NAMED_IDENTIFIERS,
        ModelContextEnvelope, named_identifiers,
    };

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn fixture_root() -> std::path::PathBuf {
        let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("aer-model-context-{now}-{nonce}"));
        fs::create_dir_all(root.join("docs")).expect("fixture docs");
        fs::create_dir_all(root.join("src")).expect("fixture src");
        fs::write(
            root.join("docs/00_READ_ME_FIRST.md"),
            "# Read first\n\n## 1. What this repository is intended to become\nAER is model agnostic.\n\n## 2. Highest-level product invariant\nOptimize verified outcome per cost.\n\n## 3. Usage\nunselected mutable prose.\n\n## 4. Authority order\nExplicit user instruction outranks repository docs.\n\n## 5. Change discipline\nunselected tail.\n",
        )
        .expect("read first");
        fs::write(
            root.join("docs/02_ARCHITECTURE_PRINCIPLES.md"),
            "# Principles\n\n## P3 — Deterministic mechanisms dominate where possible\nUse deterministic mechanisms.\n\n## P4 — Context is a scarce resource\nBudget context.\n\n## P5 — Single agent first\nOther principle.\n\n## P6 — Separate proposing from judging\nVerification is independent.\n\n## P7 — Evidence outranks narrative\nEvidence wins.\n\n## P8 — Preserve long-horizon maintainability\nOther principle.\n\n## P9 — Fail closed at trust boundaries\nAmbiguous authority fails closed.\n\n## P10 — Event-derived durable state\nOther principle.\n\n## P17 — Observability is part of correctness\nExplain major decisions.\n\n## P18 — Policies are versioned artifacts\nVersion policy.\n",
        )
        .expect("principles");
        fs::write(
            root.join("docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md"),
            "# Provider runtime\n\n## 3. Provider transport\nTransport is separate.\n\n### 3.1 Provider-local behavior isolation\nAuthentication cannot import provider-local authority.\n\n### 3.2 Other\nunselected.\n\n## 6. Permission mode is not capability authority\nPermission mode cannot widen capability.\n\n## 11. Security invariants\nProvider-native tools cannot bypass AER authority.\n\n## 12. Evolution\nunselected tail.\n",
        )
        .expect("provider runtime");
        root
    }

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .expect("git command");
        assert!(status.success());
    }

    #[test]
    fn constitutional_core_is_deterministic_and_cache_stable_for_unselected_edits() {
        let root = fixture_root();
        let first = ArchitectureContextCapsule::compile(&root).expect("first capsule");
        let second = ArchitectureContextCapsule::compile(&root).expect("second capsule");
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.version, 3);
        assert_eq!(first.source_paths().len(), 3);
        assert!(!first.rendered.contains("unselected mutable prose"));
        assert!(!first.rendered.contains("fragment_sha256"));
        assert!(!first.rendered.contains("lines:"));

        fs::write(
            root.join("docs/00_READ_ME_FIRST.md"),
            fs::read_to_string(root.join("docs/00_READ_ME_FIRST.md"))
                .expect("read")
                .replace("unselected mutable prose.", "changed but still unselected."),
        )
        .expect("rewrite");
        let unselected_change =
            ArchitectureContextCapsule::compile(&root).expect("unselected change");
        assert_eq!(first.digest, unselected_change.digest);
        assert_ne!(first.sources[0].sha256, unselected_change.sources[0].sha256);

        fs::write(
            root.join("docs/00_READ_ME_FIRST.md"),
            format!(
                "Unselected preface that shifts source lines.\n\n{}",
                fs::read_to_string(root.join("docs/00_READ_ME_FIRST.md")).expect("read")
            ),
        )
        .expect("shift selected lines");
        let shifted = ArchitectureContextCapsule::compile(&root).expect("shifted capsule");
        assert_ne!(
            unselected_change.sources[0].start_line,
            shifted.sources[0].start_line
        );
        assert_eq!(unselected_change.rendered, shifted.rendered);
        assert_eq!(unselected_change.digest, shifted.digest);

        fs::write(
            root.join("docs/00_READ_ME_FIRST.md"),
            fs::read_to_string(root.join("docs/00_READ_ME_FIRST.md"))
                .expect("read")
                .replace(
                    "AER is model agnostic.",
                    "AER is explicitly model agnostic.",
                ),
        )
        .expect("rewrite selected");
        let selected_change = ArchitectureContextCapsule::compile(&root).expect("selected change");
        assert_ne!(shifted.digest, selected_change.digest);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn provider_envelope_cache_identity_ignores_provenance_only_workspace_churn() {
        let root = fixture_root();
        fs::write(
            root.join("src/auth.rs"),
            "pub fn reject_expired_token(token: &str) -> bool { token != \"expired\" }\n",
        )
        .expect("source");
        git(&root, &["init"]);
        git(&root, &["config", "user.email", "aer@example.invalid"]);
        git(&root, &["config", "user.name", "AER Test"]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "fixture"]);

        let objective =
            "inspect expired token rejection in src/auth.rs and explain its authority boundary";
        let first =
            ModelContextEnvelope::compile(&root, objective).expect("first context envelope");
        assert!(
            first
                .rendered
                .starts_with("# everything/AER constitutional core")
        );
        assert!(
            first
                .rendered
                .contains("# Task-specific Context Economy pack")
        );
        assert!(!first.rendered.contains("repo_snapshot:"));
        assert!(!first.rendered.contains("pack_id:"));
        assert!(first.task_context.total_token_cost() <= first.dynamic_context_budget);
        assert!(first.estimated_tokens <= MAX_AER_CONTEXT_ESTIMATED_TOKENS);
        assert!(
            first
                .task_context
                .items
                .iter()
                .any(|item| item.path == "src/auth.rs")
        );

        // Binary/unretrievable workspace churn changes repository provenance,
        // but cannot become model context. This isolates the exact invariant:
        // audit identity may change while semantic prompt bytes stay identical.
        fs::write(
            root.join("cache-churn.bin"),
            [0_u8, 0xff, 0x00, 0xfe, 0x01, 0x02, 0x03],
        )
        .expect("provenance-only workspace churn");
        let second = ModelContextEnvelope::compile(&root, objective).expect("second envelope");
        assert_ne!(
            first.task_context.repo_snapshot,
            second.task_context.repo_snapshot
        );
        assert_ne!(first.task_context.pack_id, second.task_context.pack_id);
        assert_eq!(
            first
                .task_context
                .items
                .iter()
                .map(|item| (&item.path, &item.rendered_text))
                .collect::<Vec<_>>(),
            second
                .task_context
                .items
                .iter()
                .map(|item| (&item.path, &item.rendered_text))
                .collect::<Vec<_>>()
        );
        assert_eq!(first.rendered, second.rendered);
        assert_eq!(first.digest, second.digest);
        // The provider-visible split is what actually ships, so cache stability
        // has to hold layer by layer, not only on the combined rendering.
        assert_eq!(
            first.delegated_context().authority(),
            second.delegated_context().authority()
        );
        assert_eq!(
            first.delegated_context().evidence(),
            second.delegated_context().evidence()
        );
        // Authority is AER-owned policy only; selected repository evidence is
        // data and must not appear there.
        assert!(
            first
                .delegated_context()
                .authority()
                .starts_with("# everything/AER constitutional core")
        );
        assert!(
            !first
                .delegated_context()
                .authority()
                .contains("src/auth.rs")
        );
        assert!(first.delegated_context().evidence().contains("src/auth.rs"));

        fs::write(
            root.join("src/auth.rs"),
            "pub fn reject_expired_token(token: &str) -> bool { !token.is_empty() && token != \"expired\" }\n",
        )
        .expect("selected source change");
        let selected_change =
            ModelContextEnvelope::compile(&root, objective).expect("selected source envelope");
        assert_ne!(second.digest, selected_change.digest);
        assert_ne!(
            second.delegated_context().evidence(),
            selected_change.delegated_context().evidence()
        );
        assert_eq!(
            second.delegated_context().authority(),
            selected_change.delegated_context().authority(),
            "a task-evidence change must not perturb the cache-stable authority prefix"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    /// The constitutional core quotes real repository headings verbatim, so a
    /// documentation renumbering silently breaks every provider call while the
    /// synthetic fixtures above keep passing. Compile against the real docs.
    #[test]
    fn constitutional_core_compiles_against_the_real_repository_documents() {
        let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root");
        let capsule = ArchitectureContextCapsule::compile(&repository_root)
            .expect("every referenced authority section must exist in this repository");
        assert_eq!(capsule.sources.len(), super::CORE_SECTIONS.len());
        assert!(
            capsule
                .sources
                .iter()
                .all(|source| !source.text.trim().is_empty())
        );
    }

    #[test]
    fn only_code_shaped_quoted_names_become_retrieval_demands() {
        assert_eq!(
            named_identifiers(
                "what integer version does `ArchitectureContextCapsule::compile` assign?",
                MAX_NAMED_IDENTIFIERS
            ),
            vec!["ArchitectureContextCapsule::compile".to_owned()]
        );
        assert_eq!(
            named_identifiers(
                "what decimal value does `MAX_DYNAMIC_CONTEXT_BUDGET` evaluate to?",
                MAX_NAMED_IDENTIFIERS
            ),
            vec!["MAX_DYNAMIC_CONTEXT_BUDGET".to_owned()]
        );
        // Quoted answer literals are prose, not repository identifiers.
        assert!(
            named_identifiers(
                "Reply exactly `no` or `yes`, lowercase.",
                MAX_NAMED_IDENTIFIERS
            )
            .is_empty()
        );
        assert!(named_identifiers("no backticks here at all", MAX_NAMED_IDENTIFIERS).is_empty());
        assert!(
            named_identifiers("`not an identifier!`", MAX_NAMED_IDENTIFIERS).is_empty(),
            "punctuation is not an identifier"
        );
        assert_eq!(
            named_identifiers(
                "`A_one` `B_two` `C_three` `D_four` `E_five`",
                MAX_NAMED_IDENTIFIERS
            )
            .len(),
            MAX_NAMED_IDENTIFIERS
        );
    }

    #[test]
    fn provider_context_carries_the_exact_definition_a_task_names() {
        let root = fixture_root();
        let mut source = String::from(
            "//! capsule compile version assign integer compiled capsule\npub struct Capsule {\n    pub version: u32,\n}\n\npub struct Envelope {\n    pub version: u32,\n}\n\n",
        );
        for index in 0..200 {
            source.push_str(&format!("pub const PREAMBLE_{index}: u32 = {index};\n"));
        }
        source.push_str("\nimpl Capsule {\n    pub fn compile() -> Self {\n");
        for index in 0..40 {
            source.push_str(&format!("        let _filler_{index} = PREAMBLE_0;\n"));
        }
        source.push_str("        Self { version: 3 }\n    }\n}\n\nimpl Envelope {\n    pub fn compile() -> Self {\n        Self { version: 7 }\n    }\n}\n");
        fs::write(root.join("src/capsule.rs"), source).expect("capsule source");
        git(&root, &["init"]);
        git(&root, &["config", "user.email", "aer@example.invalid"]);
        git(&root, &["config", "user.name", "AER Test"]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-m", "fixture"]);

        let envelope = ModelContextEnvelope::compile(
            &root,
            "Using only the supplied AER context, what integer version does `Capsule::compile` assign to the compiled capsule?",
        )
        .expect("context envelope");
        assert!(
            envelope.rendered.contains("Self { version: 3 }"),
            "the named definition must reach the model verbatim"
        );
        assert!(
            !envelope.rendered.contains("Self { version: 7 }"),
            "the unrelated definition must not be pulled in"
        );
        assert!(envelope.task_context.total_token_cost() <= envelope.dynamic_context_budget);
        assert!(envelope.estimated_tokens <= MAX_AER_CONTEXT_ESTIMATED_TOKENS);

        // The required exact definition must survive the authority split, and it
        // must land in the untrusted data layer rather than in system authority.
        let delegated = envelope.delegated_context();
        assert!(delegated.evidence().contains("Self { version: 3 }"));
        assert!(!delegated.authority().contains("Self { version: 3 }"));
        assert!(
            delegated
                .user_layer("what version?")
                .contains("Self { version: 3 }")
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
