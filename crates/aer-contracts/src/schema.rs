//! Draft 2020-12 schema registry, compilation, and instance validation.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

use aer_domain::contracts::{CORE_CONTRACTS, CoreContract};
use jsonschema::{Registry, Validator};
use serde_json::Value;

const DRAFT_2020_12_URI: &str = "https://json-schema.org/draft/2020-12/schema";

#[derive(Debug)]
struct SchemaSource {
    contract: CoreContract,
    id: String,
    schema: Value,
}

/// Compiled validators for every architecture-defined core contract.
#[derive(Debug)]
pub struct ContractRegistry {
    validators: BTreeMap<CoreContract, Validator>,
    schema_ids: BTreeMap<CoreContract, String>,
}

impl ContractRegistry {
    /// Loads, meta-validates, registers, resolves, and compiles every shipped schema.
    pub fn load(root: &Path) -> Result<Self, SchemaRegistryError> {
        let mut sources = Vec::with_capacity(CORE_CONTRACTS.len());
        let mut ids = BTreeSet::new();

        for contract in CORE_CONTRACTS {
            let descriptor = contract.descriptor();
            let path = root.join(descriptor.schema_path);
            let text = fs::read_to_string(&path).map_err(|source| SchemaRegistryError::Io {
                path: path.clone(),
                source,
            })?;
            let schema: Value = serde_json::from_str(&text).map_err(|source| {
                SchemaRegistryError::JsonSchemaParse {
                    path: path.clone(),
                    source: source.to_string(),
                }
            })?;

            let declared_draft = schema.get("$schema").and_then(Value::as_str);
            if declared_draft != Some(DRAFT_2020_12_URI) {
                return Err(SchemaRegistryError::UnsupportedDraft {
                    contract,
                    found: declared_draft.map(str::to_owned),
                });
            }

            jsonschema::draft202012::meta::validate(&schema).map_err(|source| {
                SchemaRegistryError::MetaValidation {
                    contract,
                    source: source.to_string(),
                }
            })?;

            let id = schema
                .get("$id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or(SchemaRegistryError::MissingSchemaId { contract })?
                .to_owned();
            if !ids.insert(id.clone()) {
                return Err(SchemaRegistryError::DuplicateSchemaId(id));
            }

            sources.push(SchemaSource {
                contract,
                id,
                schema,
            });
        }

        let resources = sources
            .iter()
            .map(|source| (source.id.clone(), source.schema.clone()))
            .collect::<Vec<_>>();
        let registry = Registry::new()
            .extend(resources)
            .map_err(|source| SchemaRegistryError::ReferenceRegistry(source.to_string()))?
            .prepare()
            .map_err(|source| SchemaRegistryError::ReferenceRegistry(source.to_string()))?;

        let mut validators = BTreeMap::new();
        let mut schema_ids = BTreeMap::new();
        for source in sources {
            let validator = jsonschema::draft202012::options()
                .with_registry(&registry)
                .should_validate_formats(true)
                .build(&source.schema)
                .map_err(|error| SchemaRegistryError::Compilation {
                    contract: source.contract,
                    source: error.to_string(),
                })?;
            validators.insert(source.contract, validator);
            schema_ids.insert(source.contract, source.id);
        }

        Ok(Self {
            validators,
            schema_ids,
        })
    }

    /// Number of compiled core contracts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.validators.len()
    }

    /// Whether no contract validator is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    /// Canonical `$id` declared by a loaded contract schema.
    #[must_use]
    pub fn schema_id(&self, contract: CoreContract) -> Option<&str> {
        self.schema_ids.get(&contract).map(String::as_str)
    }

    /// Validates an instance against an explicitly declared compatibility version.
    pub fn validate(
        &self,
        contract: CoreContract,
        declared_version: u32,
        instance: &Value,
    ) -> Result<(), ContractValidationError> {
        let descriptor = contract.descriptor();
        if declared_version != descriptor.current_schema_version {
            return Err(ContractValidationError::UnsupportedVersion {
                contract,
                declared: declared_version,
                supported: descriptor.current_schema_version,
            });
        }

        if let Some(inline_version) = instance.get("schema_version").and_then(Value::as_u64)
            && inline_version != u64::from(declared_version)
        {
            return Err(ContractValidationError::InlineVersionMismatch {
                contract,
                declared: declared_version,
                inline: inline_version,
            });
        }

        let validator = self
            .validators
            .get(&contract)
            .expect("all core contracts are compiled together");
        let issues = validator
            .iter_errors(instance)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ContractValidationError::Structural { contract, issues })
        }
    }

    /// Validates against the currently supported contract version.
    pub fn validate_current(
        &self,
        contract: CoreContract,
        instance: &Value,
    ) -> Result<(), ContractValidationError> {
        self.validate(
            contract,
            contract.descriptor().current_schema_version,
            instance,
        )
    }
}

/// Reads a JSON or YAML document into the common JSON value representation.
pub fn load_document(path: &Path) -> Result<Value, DocumentError> {
    let text = fs::read_to_string(path).map_err(|source| DocumentError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let extension = path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);

    match extension.as_deref() {
        Some("json") => serde_json::from_str(&text).map_err(|source| DocumentError::Parse {
            path: path.to_path_buf(),
            format: "JSON",
            source: source.to_string(),
        }),
        Some("yaml" | "yml") => {
            serde_yaml_ng::from_str(&text).map_err(|source| DocumentError::Parse {
                path: path.to_path_buf(),
                format: "YAML",
                source: source.to_string(),
            })
        }
        _ => Err(DocumentError::UnsupportedExtension(path.to_path_buf())),
    }
}

#[derive(Debug)]
pub enum SchemaRegistryError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    JsonSchemaParse {
        path: PathBuf,
        source: String,
    },
    UnsupportedDraft {
        contract: CoreContract,
        found: Option<String>,
    },
    MissingSchemaId {
        contract: CoreContract,
    },
    DuplicateSchemaId(String),
    MetaValidation {
        contract: CoreContract,
        source: String,
    },
    ReferenceRegistry(String),
    Compilation {
        contract: CoreContract,
        source: String,
    },
}

impl std::fmt::Display for SchemaRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::JsonSchemaParse { path, source } => {
                write!(
                    formatter,
                    "invalid JSON schema {}: {source}",
                    path.display()
                )
            }
            Self::UnsupportedDraft { contract, found } => write!(
                formatter,
                "{} must declare Draft 2020-12; found {found:?}",
                contract.descriptor().canonical_name
            ),
            Self::MissingSchemaId { contract } => write!(
                formatter,
                "{} schema has no non-empty $id",
                contract.descriptor().canonical_name
            ),
            Self::DuplicateSchemaId(id) => write!(formatter, "duplicate schema $id: {id}"),
            Self::MetaValidation { contract, source } => write!(
                formatter,
                "{} fails Draft 2020-12 meta-validation: {source}",
                contract.descriptor().canonical_name
            ),
            Self::ReferenceRegistry(source) => {
                write!(formatter, "schema reference registry failed: {source}")
            }
            Self::Compilation { contract, source } => write!(
                formatter,
                "{} schema compilation failed: {source}",
                contract.descriptor().canonical_name
            ),
        }
    }
}

impl std::error::Error for SchemaRegistryError {}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ContractValidationError {
    UnsupportedVersion {
        contract: CoreContract,
        declared: u32,
        supported: u32,
    },
    InlineVersionMismatch {
        contract: CoreContract,
        declared: u32,
        inline: u64,
    },
    Structural {
        contract: CoreContract,
        issues: Vec<String>,
    },
}

impl std::fmt::Display for ContractValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion {
                contract,
                declared,
                supported,
            } => write!(
                formatter,
                "{} version {declared} is unsupported; current supported version is {supported}",
                contract.descriptor().canonical_name
            ),
            Self::InlineVersionMismatch {
                contract,
                declared,
                inline,
            } => write!(
                formatter,
                "{} declares compatibility version {declared} but object schema_version is {inline}",
                contract.descriptor().canonical_name
            ),
            Self::Structural { contract, issues } => write!(
                formatter,
                "{} structural validation failed: {}",
                contract.descriptor().canonical_name,
                issues.join("; ")
            ),
        }
    }
}

impl std::error::Error for ContractValidationError {}

#[derive(Debug)]
pub enum DocumentError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        format: &'static str,
        source: String,
    },
    UnsupportedExtension(PathBuf),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Parse {
                path,
                format,
                source,
            } => write!(
                formatter,
                "invalid {format} document {}: {source}",
                path.display()
            ),
            Self::UnsupportedExtension(path) => {
                write!(
                    formatter,
                    "unsupported document extension: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for DocumentError {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aer_domain::contracts::{CORE_CONTRACTS, CoreContract};
    use serde_json::json;

    use super::{ContractRegistry, ContractValidationError, load_document};

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn every_shipped_schema_meta_validates_and_compiles() {
        let registry = ContractRegistry::load(&repository_root()).expect("schemas should compile");
        assert_eq!(registry.len(), CORE_CONTRACTS.len());
        assert!(!registry.is_empty());
        for contract in CORE_CONTRACTS {
            assert!(registry.schema_id(contract).is_some());
        }
    }

    #[test]
    fn relative_budget_ref_is_resolved_inside_task_schema() {
        let registry = ContractRegistry::load(&repository_root()).expect("schemas should compile");
        let task = json!({
            "schema_version": 1,
            "task_id": "TASK-REF",
            "kind": "implementation",
            "objective": "Prove relative schema references are live.",
            "risk": "low",
            "budget": {"input_tokens": -1},
            "state": "ready",
            "spec_version": 1
        });

        assert!(matches!(
            registry.validate_current(CoreContract::TaskEnvelope, &task),
            Err(ContractValidationError::Structural { .. })
        ));
    }

    #[test]
    fn unsupported_contract_version_fails_closed() {
        let registry = ContractRegistry::load(&repository_root()).expect("schemas should compile");
        let config = json!({"quality_mode": "balanced"});
        assert!(matches!(
            registry.validate(CoreContract::Configuration, 2, &config),
            Err(ContractValidationError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn yaml_examples_parse_into_common_json_representation() {
        let path = repository_root().join("docs/examples/example-project-ir.yaml");
        let value = load_document(&path).expect("checked-in YAML example should parse");
        assert_eq!(value["schema_version"], 1);
    }
}
