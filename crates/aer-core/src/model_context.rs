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
use aer_repo::{IndexPolicy, RepoError, RepositoryIndex};
use sha2::{Digest, Sha256};

/// Stable constitutional context is deliberately small and cache-friendly. The
/// task-specific remainder is selected by the existing Context Economy Engine.
const ARCHITECTURE_POLICY_VERSION: &str = "architecture-context-v2";
const PROVIDER_CONTEXT_POLICY_VERSION: &str = "provider-context-economy-v1";
const MAX_STABLE_CORE_ESTIMATED_TOKENS: u32 = 12 * 1024;
const MAX_AER_CONTEXT_ESTIMATED_TOKENS: u32 = 18 * 1024;
const CONTEXT_ENVELOPE_RESERVE: u32 = 1024;
const MIN_DYNAMIC_CONTEXT_BUDGET: u32 = 2 * 1024;
const MAX_DYNAMIC_CONTEXT_BUDGET: u32 = 6 * 1024;

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
        "## 10. Security invariants",
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
    /// authoritative Markdown sections. It never byte-truncates a section: an
    /// oversized or missing constitution fails closed instead.
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
                "## Authority fragment: {}\nsection: {}\nlines: {}-{}\nfragment_sha256: {}\n",
                source.path,
                source.section,
                source.start_line,
                source.end_line,
                source.fragment_sha256,
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
            version: 2,
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelContextEnvelope {
    pub version: u32,
    pub digest: String,
    pub architecture: ArchitectureContextCapsule,
    pub task_context: ContextPack,
    pub dynamic_context_budget: u32,
    pub estimated_tokens: u32,
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
        index.refresh(&root)?;
        let objective = objective.trim();
        let task_id = format!("provider-probe:{}", &hex_sha256(objective.as_bytes())[..16]);
        let request = ContextRequest::new(task_id, objective, 1, dynamic_context_budget);
        let task_context = engine.compile(&root, &index, &request)?;
        engine.verify_fidelity(&root, &index, &task_context)?;

        let mut rendered = architecture.rendered.clone();
        use fmt::Write as _;
        writeln!(
            rendered,
            "# Task-specific Context Economy pack\npolicy: {}\npack_id: {}\nrepo_snapshot: {}\nbudget: {} estimated token units\n",
            task_context.policy_version,
            task_context.pack_id,
            task_context.repo_snapshot,
            task_context.input_token_budget,
        )
        .expect("writing to String cannot fail");
        for item in &task_context.items {
            rendered.push_str(&item.rendered_text);
            if !item.rendered_text.ends_with('\n') {
                rendered.push('\n');
            }
            rendered.push('\n');
        }
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
            version: 1,
            digest,
            architecture,
            task_context,
            dynamic_context_budget,
            estimated_tokens,
            rendered,
        })
    }
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
        ArchitectureContextCapsule, MAX_AER_CONTEXT_ESTIMATED_TOKENS, ModelContextEnvelope,
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
            "# Provider runtime\n\n## 3. Provider transport\nTransport is separate.\n\n### 3.1 Provider-local behavior isolation\nAuthentication cannot import provider-local authority.\n\n### 3.2 Other\nunselected.\n\n## 6. Permission mode is not capability authority\nPermission mode cannot widen capability.\n\n## 10. Security invariants\nProvider-native tools cannot bypass AER authority.\n\n## 11. Evolution\nunselected tail.\n",
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
        assert_eq!(first.version, 2);
        assert_eq!(first.source_paths().len(), 3);
        assert!(!first.rendered.contains("unselected mutable prose"));

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
            fs::read_to_string(root.join("docs/00_READ_ME_FIRST.md"))
                .expect("read")
                .replace(
                    "AER is model agnostic.",
                    "AER is explicitly model agnostic.",
                ),
        )
        .expect("rewrite selected");
        let selected_change = ArchitectureContextCapsule::compile(&root).expect("selected change");
        assert_ne!(first.digest, selected_change.digest);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn provider_envelope_uses_bounded_ri2_context_after_stable_prefix() {
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

        let envelope = ModelContextEnvelope::compile(
            &root,
            "inspect expired token rejection in src/auth.rs and explain its authority boundary",
        )
        .expect("context envelope");
        assert!(
            envelope
                .rendered
                .starts_with("# everything/AER constitutional core")
        );
        assert!(
            envelope
                .rendered
                .contains("# Task-specific Context Economy pack")
        );
        assert!(envelope.task_context.total_token_cost() <= envelope.dynamic_context_budget);
        assert!(envelope.estimated_tokens <= MAX_AER_CONTEXT_ESTIMATED_TOKENS);
        assert!(
            envelope
                .task_context
                .items
                .iter()
                .any(|item| item.path == "src/auth.rs")
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
