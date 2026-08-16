use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use aer_environment::EnvironmentFingerprint;
use aer_exec::{CommandSpec, ExecutionPolicy, LocalProcessExecutor, SideEffectClass};
use serde_json::Value;

use crate::{IndexPolicy, RepoError, path_string, validate_relative};

use super::model::{BuildPackage, BuildTarget, FreshnessState, ProjectDependency};
use super::stable_id;

pub(crate) const CARGO_PRODUCER: &str = "cargo-metadata";
pub(crate) const CARGO_PRODUCER_VERSION: &str = "1";

#[derive(Clone, Debug)]
pub(crate) struct BuildTopology {
    pub state: FreshnessState,
    pub environment_fingerprint: Option<String>,
    pub packages: Vec<BuildPackage>,
    pub targets: Vec<BuildTarget>,
    pub dependencies: Vec<ProjectDependency>,
}

impl BuildTopology {
    pub(crate) fn unavailable() -> Self {
        Self {
            state: FreshnessState::Unavailable,
            environment_fingerprint: None,
            packages: Vec::new(),
            targets: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    fn unavailable_in(environment_fingerprint: Option<String>) -> Self {
        Self {
            environment_fingerprint,
            ..Self::unavailable()
        }
    }
}

pub(crate) fn collect_project_topology(repo: &Path, policy: &IndexPolicy) -> BuildTopology {
    if !repo.join("Cargo.toml").is_file() {
        return BuildTopology::unavailable();
    }
    let environment_fingerprint = match EnvironmentFingerprint::discover(repo) {
        Ok(fingerprint) => Some(fingerprint.digest),
        Err(_) => return BuildTopology::unavailable(),
    };
    let Ok(execution) =
        ExecutionPolicy::trusted_workspace(repo, policy.git_timeout, policy.max_git_output_bytes)
    else {
        return BuildTopology::unavailable_in(environment_fingerprint);
    };
    let Ok(result) = LocalProcessExecutor.execute(
        &execution,
        CommandSpec::new("cargo", repo, SideEffectClass::PureRead).args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
        ]),
    ) else {
        return BuildTopology::unavailable_in(environment_fingerprint);
    };
    if !result.success || result.stdout.truncated {
        return BuildTopology::unavailable_in(environment_fingerprint);
    }
    let Ok(root) = serde_json::from_slice::<Value>(&result.stdout.preview) else {
        return BuildTopology::unavailable_in(environment_fingerprint);
    };
    parse_cargo_metadata(repo, &root, environment_fingerprint.clone())
        .unwrap_or_else(|_| BuildTopology::unavailable_in(environment_fingerprint))
}

fn parse_cargo_metadata(
    repo: &Path,
    root: &Value,
    environment_fingerprint: Option<String>,
) -> Result<BuildTopology, RepoError> {
    let workspace_members: BTreeSet<String> = root
        .get("workspace_members")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let package_values = root
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| RepoError::Integrity("cargo metadata omitted packages".to_owned()))?;
    let mut packages = Vec::new();
    let mut targets = Vec::new();
    let mut dependencies = Vec::new();
    let mut names = BTreeMap::new();

    for package in package_values {
        let id = json_string(package, "id")?;
        let name = json_string(package, "name")?;
        names.insert(name.clone(), id.clone());
        let manifest_path = relative_metadata_path(repo, &json_string(package, "manifest_path")?)?;
        packages.push(BuildPackage {
            package_id: id.clone(),
            manager: "cargo".to_owned(),
            name: name.clone(),
            version: json_string(package, "version")?,
            manifest_path: manifest_path.clone(),
            workspace_member: workspace_members.contains(&id),
        });
        if let Some(package_targets) = package.get("targets").and_then(Value::as_array) {
            for target in package_targets {
                let target_name = json_string(target, "name")?;
                let kinds = target
                    .get("kind")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                let kind = if kinds.is_empty() {
                    "unknown".to_owned()
                } else {
                    kinds.join("+")
                };
                let source_path = target
                    .get("src_path")
                    .and_then(Value::as_str)
                    .map(|path| relative_metadata_path(repo, path))
                    .transpose()?;
                targets.push(BuildTarget {
                    target_id: stable_id("build-target", &[&id, &target_name, &kind]),
                    package_id: id.clone(),
                    name: target_name,
                    kind,
                    source_path,
                });
            }
        }
        if let Some(package_dependencies) = package.get("dependencies").and_then(Value::as_array) {
            for dependency in package_dependencies {
                let target_name = json_string(dependency, "name")?;
                let dependency_kind = dependency
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("normal")
                    .to_owned();
                dependencies.push(ProjectDependency {
                    source_package_id: id.clone(),
                    target_name,
                    target_package_id: None,
                    dependency_kind,
                    manifest_path: manifest_path.clone(),
                });
            }
        }
    }
    for dependency in &mut dependencies {
        dependency.target_package_id = names.get(&dependency.target_name).cloned();
    }
    Ok(BuildTopology {
        state: FreshnessState::Current,
        environment_fingerprint,
        packages,
        targets,
        dependencies,
    })
}

fn json_string(value: &Value, key: &str) -> Result<String, RepoError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| RepoError::Integrity(format!("cargo metadata field {key} is missing")))
}

fn relative_metadata_path(repo: &Path, raw: &str) -> Result<String, RepoError> {
    let raw_path = PathBuf::from(raw);
    let candidate = if raw_path.is_absolute() {
        raw_path
    } else {
        repo.join(raw_path)
    };

    // Cargo metadata uses absolute paths. Normalize both sides through the filesystem before the
    // containment check so Windows verbatim/extended-length paths and ordinary drive paths share
    // the same representation. Canonicalization also keeps the check fail-closed for symlink
    // escapes instead of trusting a lexical prefix.
    let canonical_repo = fs::canonicalize(repo)?;
    let canonical_candidate = fs::canonicalize(&candidate)?;
    let relative = canonical_candidate
        .strip_prefix(&canonical_repo)
        .map_err(|_| {
            RepoError::Integrity(format!(
                "project metadata path escaped repository root: {raw}"
            ))
        })?;
    let normalized = path_string(relative)?;
    validate_relative(&normalized)?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "aer-ri2-build-{label}-{}-{now}-{serial}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn unavailable_topology_is_explicit_not_fabricated() {
        let topology = BuildTopology::unavailable();
        assert_eq!(topology.state, FreshnessState::Unavailable);
        assert!(topology.environment_fingerprint.is_none());
        assert!(topology.packages.is_empty());
        assert!(topology.targets.is_empty());
    }

    #[test]
    fn canonical_absolute_metadata_path_is_normalized_against_repository_root() {
        let root = temp_root("canonical-path");
        fs::create_dir_all(root.join("src")).expect("create source dir");
        let source = root.join("src/lib.rs");
        fs::write(&source, "pub fn marker() {}\n").expect("write source");
        let canonical_source = fs::canonicalize(&source).expect("canonical source");

        let relative = relative_metadata_path(&root, &canonical_source.to_string_lossy())
            .expect("normalize canonical metadata path");
        assert_eq!(relative, "src/lib.rs");

        fs::remove_dir_all(root).expect("cleanup");
    }
}
