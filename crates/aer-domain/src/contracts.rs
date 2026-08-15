//! Registry of the executable core contracts defined by the architecture.

/// A high-authority AER contract with an executable JSON Schema baseline.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CoreContract {
    EngineeringIr,
    TaskEnvelope,
    RunState,
    Budget,
    ContextPack,
    HandoffEnvelope,
    WorkResult,
    EvidenceRecord,
    ProofManifest,
    ResearchArtifact,
    EnvironmentFingerprint,
    ModelCapabilityRecord,
    PolicyArtifact,
    RunEvent,
    Configuration,
}

/// Stable metadata for one core contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractDescriptor {
    pub contract: CoreContract,
    pub canonical_name: &'static str,
    pub schema_path: &'static str,
    pub current_schema_version: u32,
}

impl CoreContract {
    /// Returns the immutable Phase-0 descriptor for this contract.
    #[must_use]
    pub const fn descriptor(self) -> ContractDescriptor {
        match self {
            Self::EngineeringIr => descriptor(self, "EngineeringIR", "docs/schemas/engineering-ir.schema.json"),
            Self::TaskEnvelope => descriptor(self, "TaskEnvelope", "docs/schemas/task.schema.json"),
            Self::RunState => descriptor(self, "RunState", "docs/schemas/run.schema.json"),
            Self::Budget => descriptor(self, "Budget", "docs/schemas/budget.schema.json"),
            Self::ContextPack => descriptor(self, "ContextPack", "docs/schemas/context-pack.schema.json"),
            Self::HandoffEnvelope => descriptor(self, "HandoffEnvelope", "docs/schemas/handoff.schema.json"),
            Self::WorkResult => descriptor(self, "WorkResult", "docs/schemas/work-result.schema.json"),
            Self::EvidenceRecord => descriptor(self, "EvidenceRecord", "docs/schemas/evidence.schema.json"),
            Self::ProofManifest => descriptor(self, "ProofManifest", "docs/schemas/proof-manifest.schema.json"),
            Self::ResearchArtifact => descriptor(self, "ResearchArtifact", "docs/schemas/research-artifact.schema.json"),
            Self::EnvironmentFingerprint => descriptor(
                self,
                "EnvironmentFingerprint",
                "docs/schemas/environment-fingerprint.schema.json",
            ),
            Self::ModelCapabilityRecord => descriptor(
                self,
                "ModelCapabilityRecord",
                "docs/schemas/model-capability.schema.json",
            ),
            Self::PolicyArtifact => descriptor(self, "PolicyArtifact", "docs/schemas/policy-artifact.schema.json"),
            Self::RunEvent => descriptor(self, "RunEvent", "docs/schemas/run-event.schema.json"),
            Self::Configuration => descriptor(self, "Configuration", "docs/schemas/config.schema.json"),
        }
    }
}

const fn descriptor(
    contract: CoreContract,
    canonical_name: &'static str,
    schema_path: &'static str,
) -> ContractDescriptor {
    ContractDescriptor {
        contract,
        canonical_name,
        schema_path,
        current_schema_version: 1,
    }
}

/// Complete registry required by `docs/44_EXECUTABLE_CONTRACTS_AND_SCHEMA_DISCIPLINE.md`.
pub const CORE_CONTRACTS: [CoreContract; 15] = [
    CoreContract::EngineeringIr,
    CoreContract::TaskEnvelope,
    CoreContract::RunState,
    CoreContract::Budget,
    CoreContract::ContextPack,
    CoreContract::HandoffEnvelope,
    CoreContract::WorkResult,
    CoreContract::EvidenceRecord,
    CoreContract::ProofManifest,
    CoreContract::ResearchArtifact,
    CoreContract::EnvironmentFingerprint,
    CoreContract::ModelCapabilityRecord,
    CoreContract::PolicyArtifact,
    CoreContract::RunEvent,
    CoreContract::Configuration,
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::CORE_CONTRACTS;

    #[test]
    fn contract_names_and_schema_paths_are_unique() {
        let mut names = BTreeSet::new();
        let mut paths = BTreeSet::new();

        for contract in CORE_CONTRACTS {
            let descriptor = contract.descriptor();
            assert!(names.insert(descriptor.canonical_name));
            assert!(paths.insert(descriptor.schema_path));
        }
    }

    #[test]
    fn every_foundation_contract_has_a_nonzero_schema_version() {
        for contract in CORE_CONTRACTS {
            assert!(contract.descriptor().current_schema_version > 0);
        }
    }
}
