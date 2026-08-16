use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

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
    pub packages: Vec<BuildPackage>,
    pub targets: Vec<BuildTarget>,
    pub dependencies: Vec<ProjectDependency>,
}

impl BuildTopology {
    pub(crate) fn unavailable() -> Self {
        Self {
            state: FreshnessState::Unavailable,
            packages: Vec::new(),
            targets: Vec::new(),
            dependencies: Vec::new(),
        }
    }
}

pub(crate) fn collect_project_topology(repo: &Path, policy: &IndexPolicy) -> BuildTopology {
    if !repo.join("Cargo.toml").is_file() {
        return BuildTopology::unavailable();
    }
    let Ok(execution) =
        ExecutionPolicy::trusted_workspace(repo, policy.git_timeout, policy.max_git_output_bytes)
    else {
        return BuildTopology::unavailable();
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
        return BuildTopology::unavailable();
    };
    if !result.success || result.stdout.truncated {
        return BuildTopology::unavailable();
    }
    let Ok(root) = serde_json::from_slice::<Value>(&result.stdout.preview) else {
        return BuildTopology::unavailable();
    };
    parse_cargo_metadata(repo, &root).unwrap_or_else(|_| BuildTopology::unavailable())
}

fn parse_cargo_metadata(repo: &Path, root: &Value) -> Result<BuildTopology, RepoError> {
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
    let path = PathBuf::from(raw);
    let relative = if path.is_absolute() {
        path.strip_prefix(repo).map_err(|_| {
            RepoError::Integrity(format!(
                "project metadata path escaped repository root: {raw}"
            ))
        })?
    } else {
        &path
    };
    let normalized = path_string(relative)?;
    validate_relative(&normalized)?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_topology_is_explicit_not_fabricated() {
        let topology = BuildTopology::unavailable();
        assert_eq!(topology.state, FreshnessState::Unavailable);
        assert!(topology.packages.is_empty());
        assert!(topology.targets.is_empty());
    }
}
