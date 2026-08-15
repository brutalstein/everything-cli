//! Executable Phase-0 acceptance gate.
//!
//! This tool composes the lower-level documentation, schema, compatibility,
//! fixture, and semantic guarantees into one deterministic repository check.

use std::{fs, path::Path};

use aer_contracts::{
    schema::{ContractRegistry, ContractValidationError, load_document},
    semantic::SemanticBundle,
};
use aer_domain::contracts::{CORE_CONTRACTS, CoreContract};
use serde_json::Value;

const SHIPPED_EXAMPLES: [(&str, CoreContract); 3] = [
    (
        "docs/examples/example-project-ir.yaml",
        CoreContract::EngineeringIr,
    ),
    (
        "docs/examples/example-handoff.json",
        CoreContract::HandoffEnvelope,
    ),
    (
        "docs/examples/example-proof-manifest.yaml",
        CoreContract::ProofManifest,
    ),
];

const STRUCTURAL_NEGATIVE_FIXTURES: [(&str, CoreContract); 3] = [
    (
        "fixtures/phase0/structural/task-unknown-field.json",
        CoreContract::TaskEnvelope,
    ),
    (
        "fixtures/phase0/structural/task-negative-budget.json",
        CoreContract::TaskEnvelope,
    ),
    (
        "fixtures/phase0/structural/config-secret-field.json",
        CoreContract::Configuration,
    ),
];

const SEMANTIC_FIXTURES: [&str; 3] = [
    "fixtures/phase0/semantic/valid-chain.json",
    "fixtures/phase0/semantic/dangling-acceptance.json",
    "fixtures/phase0/semantic/cyclic-tasks.json",
];

const COMPATIBILITY_FIXTURES: [&str; 3] = [
    "fixtures/phase0/compatibility/current-config.json",
    "fixtures/phase0/compatibility/future-config.json",
    "fixtures/phase0/compatibility/task-inline-version-mismatch.json",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Phase0Report {
    pub compiled_schemas: usize,
    pub shipped_examples: usize,
    pub structural_negative_fixtures: usize,
    pub semantic_fixtures: usize,
    pub compatibility_fixtures: usize,
    pub normative_config_blocks: usize,
}

pub fn check_repository(root: &Path) -> Result<Phase0Report, String> {
    let registry = ContractRegistry::load(root).map_err(|error| error.to_string())?;

    validate_shipped_examples(root, &registry)?;
    validate_structural_negative_fixtures(root, &registry)?;
    validate_semantic_fixtures(root, &registry)?;
    validate_compatibility_fixtures(root, &registry)?;
    let normative_config_blocks = validate_normative_config_blocks(root, &registry)?;

    Ok(Phase0Report {
        compiled_schemas: registry.len(),
        shipped_examples: SHIPPED_EXAMPLES.len(),
        structural_negative_fixtures: STRUCTURAL_NEGATIVE_FIXTURES.len(),
        semantic_fixtures: SEMANTIC_FIXTURES.len(),
        compatibility_fixtures: COMPATIBILITY_FIXTURES.len(),
        normative_config_blocks,
    })
}

fn validate_shipped_examples(root: &Path, registry: &ContractRegistry) -> Result<(), String> {
    for (path, contract) in SHIPPED_EXAMPLES {
        let instance = load_document(&root.join(path)).map_err(|error| error.to_string())?;
        registry
            .validate_current(contract, &instance)
            .map_err(|error| format!("shipped example {path}: {error}"))?;
    }
    Ok(())
}

fn validate_structural_negative_fixtures(
    root: &Path,
    registry: &ContractRegistry,
) -> Result<(), String> {
    for (path, contract) in STRUCTURAL_NEGATIVE_FIXTURES {
        let instance = load_document(&root.join(path)).map_err(|error| error.to_string())?;
        match registry.validate_current(contract, &instance) {
            Err(ContractValidationError::Structural { .. }) => {}
            Ok(()) => return Err(format!("negative fixture unexpectedly validated: {path}")),
            Err(error) => {
                return Err(format!(
                    "negative fixture {path} failed at the wrong validation layer: {error}"
                ));
            }
        }
    }
    Ok(())
}

fn validate_semantic_fixtures(root: &Path, registry: &ContractRegistry) -> Result<(), String> {
    for path in SEMANTIC_FIXTURES {
        let fixture = load_document(&root.join(path)).map_err(|error| error.to_string())?;
        let expectation = fixture
            .get("expect")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("semantic fixture {path} has no expect string"))?;
        let bundle_value = fixture
            .get("bundle")
            .ok_or_else(|| format!("semantic fixture {path} has no bundle"))?;
        let bundle = semantic_bundle_from_value(path, bundle_value)?;

        validate_bundle_structure(path, registry, &bundle)?;
        let issues = bundle.validate();
        if expectation == "pass" {
            if !issues.is_empty() {
                return Err(format!(
                    "semantic fixture {path} expected pass but produced {issues:?}"
                ));
            }
        } else if !issues.iter().any(|issue| issue.code == expectation) {
            return Err(format!(
                "semantic fixture {path} expected issue {expectation}, got {issues:?}"
            ));
        }
    }
    Ok(())
}

fn semantic_bundle_from_value(path: &str, value: &Value) -> Result<SemanticBundle, String> {
    let engineering_ir = value
        .get("engineering_ir")
        .cloned()
        .ok_or_else(|| format!("semantic fixture {path} has no engineering_ir"))?;
    let tasks = value_array(path, value, "tasks")?;
    let evidence = value_array(path, value, "evidence")?;
    let proof_manifests = value_array(path, value, "proof_manifests")?;

    Ok(SemanticBundle {
        engineering_ir,
        tasks,
        evidence,
        proof_manifests,
    })
}

fn value_array(path: &str, value: &Value, key: &str) -> Result<Vec<Value>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| format!("semantic fixture {path} has no {key} array"))
}

fn validate_bundle_structure(
    path: &str,
    registry: &ContractRegistry,
    bundle: &SemanticBundle,
) -> Result<(), String> {
    registry
        .validate_current(CoreContract::EngineeringIr, &bundle.engineering_ir)
        .map_err(|error| format!("semantic fixture {path} IR is structurally invalid: {error}"))?;
    validate_instances(
        path,
        registry,
        CoreContract::TaskEnvelope,
        &bundle.tasks,
    )?;
    validate_instances(
        path,
        registry,
        CoreContract::EvidenceRecord,
        &bundle.evidence,
    )?;
    validate_instances(
        path,
        registry,
        CoreContract::ProofManifest,
        &bundle.proof_manifests,
    )
}

fn validate_instances(
    fixture_path: &str,
    registry: &ContractRegistry,
    contract: CoreContract,
    instances: &[Value],
) -> Result<(), String> {
    for (index, instance) in instances.iter().enumerate() {
        registry.validate_current(contract, instance).map_err(|error| {
            format!(
                "semantic fixture {fixture_path} {}[{index}] is structurally invalid: {error}",
                contract.descriptor().canonical_name
            )
        })?;
    }
    Ok(())
}

fn validate_compatibility_fixtures(
    root: &Path,
    registry: &ContractRegistry,
) -> Result<(), String> {
    for path in COMPATIBILITY_FIXTURES {
        let fixture = load_document(&root.join(path)).map_err(|error| error.to_string())?;
        let contract_name = required_str(path, &fixture, "contract")?;
        let contract = contract_by_name(contract_name)
            .ok_or_else(|| format!("compatibility fixture {path}: unknown contract {contract_name}"))?;
        let declared_version = fixture
            .get("declared_version")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| format!("compatibility fixture {path}: invalid declared_version"))?;
        let expectation = required_str(path, &fixture, "expect")?;
        let instance = fixture
            .get("instance")
            .ok_or_else(|| format!("compatibility fixture {path}: missing instance"))?;

        let result = registry.validate(contract, declared_version, instance);
        let matched = match expectation {
            "pass" => result.is_ok(),
            "unsupported_version" => matches!(
                result,
                Err(ContractValidationError::UnsupportedVersion { .. })
            ),
            "inline_version_mismatch" => matches!(
                result,
                Err(ContractValidationError::InlineVersionMismatch { .. })
            ),
            other => return Err(format!("compatibility fixture {path}: unknown expect {other}")),
        };
        if !matched {
            return Err(format!(
                "compatibility fixture {path} did not produce expected result {expectation}: {result:?}"
            ));
        }
    }
    Ok(())
}

fn required_str<'a>(path: &str, value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("fixture {path}: missing string {key}"))
}

fn contract_by_name(name: &str) -> Option<CoreContract> {
    CORE_CONTRACTS
        .into_iter()
        .find(|contract| contract.descriptor().canonical_name == name)
}

fn validate_normative_config_blocks(
    root: &Path,
    registry: &ContractRegistry,
) -> Result<usize, String> {
    let path = root.join("docs/29_CONFIGURATION_AND_POLICY_MODEL.md");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let blocks = fenced_blocks(&text, "yaml")?;
    if blocks.is_empty() {
        return Err("configuration documentation contains no normative YAML blocks".to_owned());
    }

    for (index, block) in blocks.iter().enumerate() {
        let config: Value = serde_yaml_ng::from_str(block).map_err(|error| {
            format!("configuration documentation YAML block {index} does not parse: {error}")
        })?;
        registry
            .validate_current(CoreContract::Configuration, &config)
            .map_err(|error| {
                format!("configuration documentation YAML block {index} is rejected: {error}")
            })?;
    }
    Ok(blocks.len())
}

fn fenced_blocks<'a>(markdown: &'a str, language: &str) -> Result<Vec<String>, String> {
    let opening = format!("```{language}");
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&'a str>> = None;

    for line in markdown.lines() {
        if current.is_none() && line.trim() == opening {
            current = Some(Vec::new());
            continue;
        }
        if let Some(lines) = current.as_mut() {
            if line.trim() == "```" {
                let completed = current
                    .take()
                    .expect("an open fenced block must contain a buffer");
                blocks.push(completed.join("\n"));
            } else {
                lines.push(line);
            }
        }
    }

    if current.is_some() {
        return Err(format!("unterminated {language} fenced block"));
    }
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{check_repository, fenced_blocks};

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn checked_in_phase0_contract_gate_is_green() {
        let report = check_repository(&repository_root())
            .expect("checked-in Phase-0 contract fixtures should satisfy all gates");
        assert_eq!(report.compiled_schemas, 15);
        assert_eq!(report.shipped_examples, 3);
        assert_eq!(report.structural_negative_fixtures, 3);
        assert_eq!(report.semantic_fixtures, 3);
        assert_eq!(report.compatibility_fixtures, 3);
        assert_eq!(report.normative_config_blocks, 2);
    }

    #[test]
    fn fenced_block_parser_fails_closed_on_unterminated_input() {
        assert!(fenced_blocks("```yaml\na: 1", "yaml").is_err());
    }
}
