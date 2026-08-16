use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

const REQUIRED_DOCS: &[&str] = &[
    "00_READ_ME_FIRST.md",
    "01_PRODUCT_THESIS_AND_NON_GOALS.md",
    "02_ARCHITECTURE_PRINCIPLES.md",
    "03_SYSTEM_ARCHITECTURE.md",
    "04_INTENT_AND_REQUIREMENTS_ENGINE.md",
    "05_ENGINEERING_IR.md",
    "06_REPOSITORY_INTELLIGENCE.md",
    "07_CONTEXT_ECONOMY_ENGINE.md",
    "08_MODEL_CAPABILITY_REGISTRY.md",
    "09_ADAPTIVE_MODEL_ROUTER_AND_BUDGETS.md",
    "10_TASK_GRAPH_AND_ORCHESTRATION.md",
    "11_PARALLELISM_WORKTREES_AND_INTEGRATION.md",
    "12_HANDOFF_ABI_AND_COGNITIVE_ADAPTERS.md",
    "13_EXECUTION_SANDBOX_AND_TOOL_RUNTIME.md",
    "14_TOOLS_MCP_A2A_AND_SKILLS.md",
    "15_ENGINEERING_STATE_AND_MEMORY.md",
    "16_FAILURE_DETECTION_AND_RECOVERY.md",
    "17_VERIFICATION_AND_PROOF_CARRYING_CHANGES.md",
    "18_ARCHITECTURE_HEALTH_CONTROLLER.md",
    "19_SECURITY_THREAT_MODEL.md",
    "20_OBSERVABILITY_AND_COST_ACCOUNTING.md",
    "21_EVALUATION_AND_BENCHMARK_STRATEGY.md",
    "22_SELF_EVOLUTION_AND_POLICY_LAB.md",
    "23_CLI_AND_USER_EXPERIENCE.md",
    "24_STORAGE_EVENT_MODEL_AND_REPLAY.md",
    "25_IMPLEMENTATION_ROADMAP.md",
    "26_AGENT_IMPLEMENTATION_PROTOCOL.md",
    "27_TARGET_REPOSITORY_STRUCTURE.md",
    "28_RESEARCH_EVIDENCE.md",
    "29_CONFIGURATION_AND_POLICY_MODEL.md",
    "30_OPEN_QUESTIONS_AND_DECISION_GATES.md",
    "31_GLOSSARY.md",
    "32_FINAL_ARCHITECTURE_DECISION_MATRIX.md",
    "33_END_TO_END_RUNTIME_SEQUENCE.md",
    "34_CORE_INVARIANTS_AND_PROPERTY_TESTS.md",
    "35_ARCHITECTURE_COMPLETENESS_AUDIT.md",
    "36_RESEARCH_AND_EXTERNAL_KNOWLEDGE.md",
    "37_PROVIDER_GATEWAY_AND_RESILIENCE.md",
    "38_ENVIRONMENT_REPRODUCIBILITY_AND_SUPPLY_CHAIN.md",
    "39_SCHEDULER_RESOURCE_GOVERNOR_AND_BACKPRESSURE.md",
    "40_VERSIONING_MIGRATIONS_AND_RELEASE_SAFETY.md",
    "41_WORKSPACE_VCS_AND_CHANGE_LIFECYCLE.md",
    "42_DATA_GOVERNANCE_RETENTION_AND_TENANCY.md",
    "43_DOMAIN_CAPABILITY_AND_VERIFICATION_PROFILES.md",
    "44_EXECUTABLE_CONTRACTS_AND_SCHEMA_DISCIPLINE.md",
];

const REQUIRED_ADRS: &[&str] = &[
    "ADR-0001-rust-local-first-core.md",
    "ADR-0002-engineering-ir-not-transcript.md",
    "ADR-0003-default-single-agent-dynamic-parallelism.md",
    "ADR-0004-verification-independent-authority.md",
    "ADR-0005-sqlite-event-journal.md",
    "ADR-0006-internal-abis-external-standards.md",
    "ADR-0007-bounded-resource-admission.md",
    "ADR-0008-explicit-compatibility-and-migrations.md",
    "ADR-0009-external-research-is-evidence-not-authority.md",
];

const CORE_SCHEMAS: &[&str] = &[
    "engineering-ir.schema.json",
    "task.schema.json",
    "handoff.schema.json",
    "work-result.schema.json",
    "evidence.schema.json",
    "proof-manifest.schema.json",
    "budget.schema.json",
    "model-capability.schema.json",
    "config.schema.json",
    "policy-artifact.schema.json",
    "run.schema.json",
    "run-event.schema.json",
    "research-artifact.schema.json",
    "environment-fingerprint.schema.json",
    "context-pack.schema.json",
];

const REQUIRED_EXAMPLES: &[&str] = &[
    "example-project-ir.yaml",
    "example-handoff.json",
    "example-proof-manifest.yaml",
];

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IntegrityReport {
    pub numbered_docs: usize,
    pub accepted_adrs: usize,
    pub core_schemas: usize,
    pub examples: usize,
    pub manifest_entries: usize,
}

pub fn check_repository(root: &Path) -> Result<IntegrityReport, IntegrityError> {
    let docs = root.join("docs");
    require_entries(&docs, REQUIRED_DOCS)?;
    require_entries(&docs.join("adrs"), REQUIRED_ADRS)?;
    require_entries(&docs.join("schemas"), CORE_SCHEMAS)?;
    require_entries(&docs.join("examples"), REQUIRED_EXAMPLES)?;

    validate_json_files(&docs.join("schemas"), CORE_SCHEMAS)?;
    validate_examples(&docs.join("examples"), REQUIRED_EXAMPLES)?;
    validate_manifest(root, &docs).map(|manifest_entries| IntegrityReport {
        numbered_docs: REQUIRED_DOCS.len(),
        accepted_adrs: REQUIRED_ADRS.len(),
        core_schemas: CORE_SCHEMAS.len(),
        examples: REQUIRED_EXAMPLES.len(),
        manifest_entries,
    })
}

pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root must exist")
}

#[derive(Debug)]
pub enum IntegrityError {
    MissingEntry {
        path: PathBuf,
    },
    InvalidJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidYaml {
        path: PathBuf,
        source: serde_yaml_ng::Error,
    },
    InvalidManifestLine {
        line: usize,
        text: String,
    },
    DuplicateManifestPath(String),
    ManifestCoverageMismatch {
        missing: Vec<String>,
        stale: Vec<String>,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEntry { path } => write!(formatter, "required docs entry missing: {}", path.display()),
            Self::InvalidJson { path, source } => {
                write!(formatter, "invalid JSON {}: {source}", path.display())
            }
            Self::InvalidYaml { path, source } => {
                write!(formatter, "invalid YAML {}: {source}", path.display())
            }
            Self::InvalidManifestLine { line, text } => {
                write!(formatter, "invalid manifest line {line}: {text}")
            }
            Self::DuplicateManifestPath(path) => {
                write!(formatter, "duplicate manifest path: {path}")
            }
            Self::ManifestCoverageMismatch { missing, stale } => write!(
                formatter,
                "docs manifest coverage mismatch; missing={missing:?}, stale={stale:?}"
            ),
            Self::Io { path, source } => write!(formatter, "I/O {}: {source}", path.display()),
        }
    }
}

impl Error for IntegrityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidJson { source, .. } => Some(source),
            Self::InvalidYaml { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            Self::MissingEntry { .. }
            | Self::InvalidManifestLine { .. }
            | Self::DuplicateManifestPath(_)
            | Self::ManifestCoverageMismatch { .. } => None,
        }
    }
}

fn require_entries(directory: &Path, entries: &[&str]) -> Result<(), IntegrityError> {
    for entry in entries {
        let path = directory.join(entry);
        if !path.is_file() {
            return Err(IntegrityError::MissingEntry { path });
        }
    }
    Ok(())
}

fn validate_json_files(directory: &Path, entries: &[&str]) -> Result<(), IntegrityError> {
    for entry in entries {
        let path = directory.join(entry);
        let bytes = fs::read(&path).map_err(|source| IntegrityError::Io {
            path: path.clone(),
            source,
        })?;
        serde_json::from_slice::<Value>(&bytes)
            .map_err(|source| IntegrityError::InvalidJson { path, source })?;
    }
    Ok(())
}

fn validate_examples(directory: &Path, entries: &[&str]) -> Result<(), IntegrityError> {
    for entry in entries {
        let path = directory.join(entry);
        let bytes = fs::read(&path).map_err(|source| IntegrityError::Io {
            path: path.clone(),
            source,
        })?;
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("json") => {
                serde_json::from_slice::<Value>(&bytes)
                    .map_err(|source| IntegrityError::InvalidJson { path, source })?;
            }
            Some("yaml" | "yml") => {
                serde_yaml_ng::from_slice::<Value>(&bytes)
                    .map_err(|source| IntegrityError::InvalidYaml { path, source })?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_manifest(root: &Path, docs: &Path) -> Result<usize, IntegrityError> {
    let manifest_path = docs.join("MANIFEST.sha256");
    let manifest = fs::read_to_string(&manifest_path).map_err(|source| IntegrityError::Io {
        path: manifest_path.clone(),
        source,
    })?;

    let mut manifest_paths = BTreeSet::new();
    let mut manifest_entries = BTreeMap::new();
    for (index, raw_line) in manifest.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((_digest, path)) = line.split_once("  ") else {
            return Err(IntegrityError::InvalidManifestLine {
                line: index + 1,
                text: raw_line.to_owned(),
            });
        };
        if !manifest_paths.insert(path.to_owned()) {
            return Err(IntegrityError::DuplicateManifestPath(path.to_owned()));
        }
        manifest_entries.insert(path.to_owned(), index + 1);
    }

    let mut actual_paths = BTreeSet::new();
    collect_docs_files(root, docs, &mut actual_paths)?;
    actual_paths.remove("docs/MANIFEST.sha256");

    let missing: Vec<_> = actual_paths.difference(&manifest_paths).cloned().collect();
    let stale: Vec<_> = manifest_paths.difference(&actual_paths).cloned().collect();
    if !missing.is_empty() || !stale.is_empty() {
        return Err(IntegrityError::ManifestCoverageMismatch { missing, stale });
    }

    Ok(manifest_paths.len())
}

fn collect_docs_files(
    root: &Path,
    directory: &Path,
    paths: &mut BTreeSet<String>,
) -> Result<(), IntegrityError> {
    let entries = fs::read_dir(directory).map_err(|source| IntegrityError::Io {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| IntegrityError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| IntegrityError::Io {
            path: entry.path(),
            source,
        })?;
        if file_type.is_dir() {
            collect_docs_files(root, &entry.path(), paths)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("walked path must remain under repository root")
                .to_string_lossy()
                .replace('\\', "/");
            paths.insert(relative);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_repository, repository_root};

    #[test]
    fn checked_in_docs_inventory_is_consistent() {
        let report = check_repository(&repository_root())
            .expect("checked-in architecture documentation should be internally consistent");

        assert_eq!(report.numbered_docs, 45);
        assert_eq!(report.accepted_adrs, 9);
        assert_eq!(report.core_schemas, 15);
        assert_eq!(report.examples, 3);
        assert_eq!(report.manifest_entries, 73);
    }
}
