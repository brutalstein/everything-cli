//! Build-embedded executable contract registry for installed runtime use.
//!
//! `schema::ContractRegistry::load` remains the repository-integrity path. This
//! registry embeds the exact checked-in schemas so an installed `everything`
//! binary can validate its own wire/domain contracts while operating on an
//! arbitrary user repository that does not contain AER's `docs/` tree.

use std::{collections::{BTreeMap, BTreeSet}, error::Error, fmt};

use aer_domain::contracts::{CORE_CONTRACTS, CoreContract};
use jsonschema::{Registry, Validator};
use serde_json::Value;

const DRAFT_2020_12_URI: &str = "https://json-schema.org/draft/2020-12/schema";

#[derive(Debug)]
struct Source {
    contract: CoreContract,
    id: String,
    schema: Value,
}

#[derive(Debug)]
pub struct EmbeddedContractRegistry {
    validators: BTreeMap<CoreContract, Validator>,
}

impl EmbeddedContractRegistry {
    pub fn load() -> Result<Self, EmbeddedRegistryError> {
        let mut sources = Vec::with_capacity(CORE_CONTRACTS.len());
        let mut ids = BTreeSet::new();
        for contract in CORE_CONTRACTS {
            let text = embedded_schema(contract);
            let schema: Value = serde_json::from_str(text).map_err(|error| {
                EmbeddedRegistryError::InvalidSchema {
                    contract,
                    message: error.to_string(),
                }
            })?;
            if schema.get("$schema").and_then(Value::as_str) != Some(DRAFT_2020_12_URI) {
                return Err(EmbeddedRegistryError::InvalidSchema {
                    contract,
                    message: "schema is not Draft 2020-12".to_owned(),
                });
            }
            jsonschema::draft202012::meta::validate(&schema).map_err(|error| {
                EmbeddedRegistryError::InvalidSchema {
                    contract,
                    message: error.to_string(),
                }
            })?;
            let id = schema
                .get("$id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| EmbeddedRegistryError::InvalidSchema {
                    contract,
                    message: "schema has no $id".to_owned(),
                })?
                .to_owned();
            if !ids.insert(id.clone()) {
                return Err(EmbeddedRegistryError::InvalidSchema {
                    contract,
                    message: format!("duplicate embedded schema id: {id}"),
                });
            }
            sources.push(Source { contract, id, schema });
        }

        let registry = Registry::new()
            .extend(
                sources
                    .iter()
                    .map(|source| (source.id.clone(), source.schema.clone())),
            )
            .map_err(|error| EmbeddedRegistryError::Registry(error.to_string()))?
            .prepare()
            .map_err(|error| EmbeddedRegistryError::Registry(error.to_string()))?;

        let mut validators = BTreeMap::new();
        for source in sources {
            let validator = jsonschema::draft202012::options()
                .with_registry(&registry)
                .should_validate_formats(true)
                .build(&source.schema)
                .map_err(|error| EmbeddedRegistryError::InvalidSchema {
                    contract: source.contract,
                    message: error.to_string(),
                })?;
            validators.insert(source.contract, validator);
        }
        Ok(Self { validators })
    }

    pub fn validate_current(
        &self,
        contract: CoreContract,
        instance: &Value,
    ) -> Result<(), EmbeddedValidationError> {
        let expected = contract.descriptor().current_schema_version;
        if instance.get("schema_version").and_then(Value::as_u64) != Some(u64::from(expected)) {
            return Err(EmbeddedValidationError {
                contract,
                issues: vec![format!("schema_version must equal {expected}")],
            });
        }
        let validator = self
            .validators
            .get(&contract)
            .expect("every core contract is compiled together");
        let issues = validator
            .iter_errors(instance)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        if issues.is_empty() {
            Ok(())
        } else {
            Err(EmbeddedValidationError { contract, issues })
        }
    }
}

#[derive(Debug)]
pub enum EmbeddedRegistryError {
    InvalidSchema { contract: CoreContract, message: String },
    Registry(String),
}

impl fmt::Display for EmbeddedRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSchema { contract, message } => {
                write!(formatter, "embedded {contract:?} schema is invalid: {message}")
            }
            Self::Registry(message) => write!(formatter, "embedded schema registry: {message}"),
        }
    }
}

impl Error for EmbeddedRegistryError {}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EmbeddedValidationError {
    pub contract: CoreContract,
    pub issues: Vec<String>,
}

impl fmt::Display for EmbeddedValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?} structural validation failed", self.contract)?;
        for issue in &self.issues {
            write!(formatter, "; {issue}")?;
        }
        Ok(())
    }
}

impl Error for EmbeddedValidationError {}

const fn embedded_schema(contract: CoreContract) -> &'static str {
    match contract {
        CoreContract::EngineeringIr => include_str!("../../../docs/schemas/engineering-ir.schema.json"),
        CoreContract::TaskEnvelope => include_str!("../../../docs/schemas/task.schema.json"),
        CoreContract::RunState => include_str!("../../../docs/schemas/run.schema.json"),
        CoreContract::Budget => include_str!("../../../docs/schemas/budget.schema.json"),
        CoreContract::ContextPack => include_str!("../../../docs/schemas/context-pack.schema.json"),
        CoreContract::HandoffEnvelope => include_str!("../../../docs/schemas/handoff.schema.json"),
        CoreContract::WorkResult => include_str!("../../../docs/schemas/work-result.schema.json"),
        CoreContract::EvidenceRecord => include_str!("../../../docs/schemas/evidence.schema.json"),
        CoreContract::ProofManifest => include_str!("../../../docs/schemas/proof-manifest.schema.json"),
        CoreContract::ResearchArtifact => include_str!("../../../docs/schemas/research-artifact.schema.json"),
        CoreContract::EnvironmentFingerprint => include_str!("../../../docs/schemas/environment-fingerprint.schema.json"),
        CoreContract::ModelCapabilityRecord => include_str!("../../../docs/schemas/model-capability.schema.json"),
        CoreContract::PolicyArtifact => include_str!("../../../docs/schemas/policy-artifact.schema.json"),
        CoreContract::RunEvent => include_str!("../../../docs/schemas/run-event.schema.json"),
        CoreContract::Configuration => include_str!("../../../docs/schemas/config.schema.json"),
    }
}

#[cfg(test)]
mod tests {
    use aer_domain::contracts::{CORE_CONTRACTS, CoreContract};
    use serde_json::json;

    use super::EmbeddedContractRegistry;

    #[test]
    fn all_checked_in_contracts_compile_from_embedded_sources() {
        let registry = EmbeddedContractRegistry::load().expect("embedded registry");
        for contract in CORE_CONTRACTS {
            assert!(registry.validators.contains_key(&contract));
        }
    }

    #[test]
    fn engineering_ir_can_be_validated_without_target_repo_docs() {
        let registry = EmbeddedContractRegistry::load().expect("embedded registry");
        let ir = json!({
            "schema_version": 1,
            "project": {"id":"p","title":"p","summary":"goal"},
            "goals": [],
            "functional_requirements": [],
            "constraints": [],
            "invariants": [],
            "acceptance_criteria": []
        });
        registry
            .validate_current(CoreContract::EngineeringIr, &ir)
            .expect("valid IR");
    }
}
