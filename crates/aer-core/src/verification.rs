//! Independent verification, evidence binding, and proof-carrying acceptance.
//!
//! This is the first architecture-complete Step-10 vertical slice. It deliberately
//! stays inside `aer-core` until verifier ownership/dependency pressure justifies a
//! separate crate. The module treats verifier definitions/assets as authority,
//! binds command evidence to exact repository + environment identity, composes
//! mandatory/domain gates monotonically, and persists accepted proof before the
//! task can enter `accepted`.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use aer_contracts::{
    embedded::EmbeddedContractRegistry, semantic::SemanticBundle, validate_semantic_bundle,
};
use aer_domain::{
    contracts::CoreContract,
    state_machines::{TaskState, TaskTransitionContext},
};
use aer_environment::{EnvironmentFingerprint, evidence::CommandExecutionEvidence};
use aer_exec::{
    CommandSpec, ExecutionPolicy, LocalProcessExecutor, SecurityProfile, SideEffectClass,
    lowercase_hex,
};
use aer_storage::{
    Causation, DurableState, EventPayload, NewEvent, ObjectHash, ObjectMetadata, Sensitivity,
    StoredEvent,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use ulid::Ulid;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum VerificationLayer {
    Mechanical,
    LocalBehavior,
    Integration,
    NonFunctional,
    Architecture,
    SemanticReview,
}

impl VerificationLayer {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Mechanical => "v0_mechanical",
            Self::LocalBehavior => "v1_local_behavior",
            Self::Integration => "v2_integration",
            Self::NonFunctional => "v3_non_functional",
            Self::Architecture => "v4_architecture",
            Self::SemanticReview => "v5_semantic_review",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EvidenceType {
    Command,
    Test,
    StaticAnalysis,
    Security,
    Performance,
    Architecture,
    SemanticReview,
    RuntimeTrace,
    Manual,
}

impl EvidenceType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Test => "test",
            Self::StaticAnalysis => "static_analysis",
            Self::Security => "security",
            Self::Performance => "performance",
            Self::Architecture => "architecture",
            Self::SemanticReview => "semantic_review",
            Self::RuntimeTrace => "runtime_trace",
            Self::Manual => "manual",
        }
    }
}

#[derive(Clone, Debug)]
pub struct VerifierDefinition {
    pub verifier_id: String,
    pub version: u32,
    pub layer: VerificationLayer,
    pub evidence_type: EvidenceType,
    pub executable: String,
    pub args: Vec<String>,
    pub protected_paths: Vec<PathBuf>,
    pub timeout: Duration,
    pub max_capture_bytes: usize,
    pub require_strong_isolation: bool,
}

impl VerifierDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        verifier_id: impl Into<String>,
        version: u32,
        layer: VerificationLayer,
        evidence_type: EvidenceType,
        executable: impl Into<String>,
        args: Vec<String>,
        protected_paths: Vec<PathBuf>,
        timeout: Duration,
        max_capture_bytes: usize,
        require_strong_isolation: bool,
    ) -> Result<Self, VerificationError> {
        let verifier_id = verifier_id.into();
        let executable = executable.into();
        if verifier_id.trim().is_empty() || executable.trim().is_empty() || version == 0 {
            return Err(VerificationError::InvalidVerifierDefinition);
        }
        if protected_paths.is_empty() || timeout.is_zero() || max_capture_bytes == 0 {
            return Err(VerificationError::InvalidVerifierDefinition);
        }
        let mut seen = BTreeSet::new();
        for path in &protected_paths {
            validate_relative_path(path)?;
            let key = portable_path(path)?;
            if !seen.insert(key) {
                return Err(VerificationError::DuplicateProtectedPath(path.clone()));
            }
        }
        Ok(Self {
            verifier_id,
            version,
            layer,
            evidence_type,
            executable,
            args,
            protected_paths,
            timeout,
            max_capture_bytes,
            require_strong_isolation,
        })
    }

    fn policy_digest(&self) -> String {
        let mut hasher = Sha256::new();
        hash_field(&mut hasher, b"AER_VERIFIER_DEFINITION_V1");
        hash_field(&mut hasher, self.verifier_id.as_bytes());
        hash_field(&mut hasher, self.version.to_string().as_bytes());
        hash_field(&mut hasher, self.layer.as_str().as_bytes());
        hash_field(&mut hasher, self.evidence_type.as_str().as_bytes());
        hash_field(&mut hasher, self.executable.as_bytes());
        for arg in &self.args {
            hash_field(&mut hasher, arg.as_bytes());
        }
        for path in &self.protected_paths {
            hash_field(
                &mut hasher,
                portable_path(path)
                    .expect("validated verifier path remains portable")
                    .as_bytes(),
            );
        }
        hash_field(
            &mut hasher,
            if self.require_strong_isolation {
                b"strong"
            } else {
                b"direct"
            },
        );
        lowercase_hex(hasher.finalize().as_ref())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifierSnapshot {
    pub verifier_id: String,
    pub verifier_version: u32,
    pub definition_digest: String,
    pub snapshot_digest: String,
    pub assets: BTreeMap<String, String>,
}

impl VerifierSnapshot {
    pub fn capture(
        definition: &VerifierDefinition,
        authority_root: impl AsRef<Path>,
    ) -> Result<Self, VerificationError> {
        let authority_root = canonical_directory(authority_root.as_ref())?;
        let assets = collect_protected_assets(&authority_root, &definition.protected_paths)?;
        let definition_digest = definition.policy_digest();
        let snapshot_digest = snapshot_digest(&definition_digest, &assets);
        Ok(Self {
            verifier_id: definition.verifier_id.clone(),
            verifier_version: definition.version,
            definition_digest,
            snapshot_digest,
            assets,
        })
    }

    pub fn assert_candidate_unchanged(
        &self,
        definition: &VerifierDefinition,
        candidate_root: impl AsRef<Path>,
    ) -> Result<(), VerificationError> {
        if self.verifier_id != definition.verifier_id
            || self.verifier_version != definition.version
            || self.definition_digest != definition.policy_digest()
        {
            return Err(VerificationError::VerifierDefinitionChanged {
                verifier_id: definition.verifier_id.clone(),
            });
        }
        let candidate = Self::capture(definition, candidate_root)?;
        if candidate.snapshot_digest != self.snapshot_digest || candidate.assets != self.assets {
            return Err(VerificationError::VerifierIntegrityViolation {
                verifier_id: definition.verifier_id.clone(),
                expected: self.snapshot_digest.clone(),
                actual: candidate.snapshot_digest,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainProfile {
    pub profile_id: String,
    pub version: u32,
    pub required_verifiers: BTreeSet<String>,
    pub required_evidence_types: BTreeSet<EvidenceType>,
}

impl DomainProfile {
    pub fn new(
        profile_id: impl Into<String>,
        version: u32,
        required_verifiers: impl IntoIterator<Item = String>,
        required_evidence_types: impl IntoIterator<Item = EvidenceType>,
    ) -> Result<Self, VerificationError> {
        let profile_id = profile_id.into();
        if profile_id.trim().is_empty() || version == 0 {
            return Err(VerificationError::InvalidDomainProfile);
        }
        let required_verifiers = required_verifiers.into_iter().collect::<BTreeSet<_>>();
        if required_verifiers.iter().any(|id| id.trim().is_empty()) {
            return Err(VerificationError::InvalidDomainProfile);
        }
        Ok(Self {
            profile_id,
            version,
            required_verifiers,
            required_evidence_types: required_evidence_types.into_iter().collect(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationPlan {
    pub required_verifiers: BTreeSet<String>,
    pub required_evidence_types: BTreeSet<EvidenceType>,
    pub profile_refs: Vec<String>,
}

impl VerificationPlan {
    pub fn compose(
        mandatory_verifiers: impl IntoIterator<Item = String>,
        mandatory_evidence_types: impl IntoIterator<Item = EvidenceType>,
        profiles: &[DomainProfile],
    ) -> Result<Self, VerificationError> {
        let mut required_verifiers = mandatory_verifiers.into_iter().collect::<BTreeSet<_>>();
        if required_verifiers.iter().any(|id| id.trim().is_empty()) {
            return Err(VerificationError::InvalidVerificationPlan);
        }
        let mut required_evidence_types = mandatory_evidence_types
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut profile_refs = Vec::new();
        for profile in profiles {
            if profile.profile_id.trim().is_empty() || profile.version == 0 {
                return Err(VerificationError::InvalidDomainProfile);
            }
            required_verifiers.extend(profile.required_verifiers.iter().cloned());
            required_evidence_types.extend(profile.required_evidence_types.iter().copied());
            profile_refs.push(format!("{}@{}", profile.profile_id, profile.version));
        }
        profile_refs.sort();
        Ok(Self {
            required_verifiers,
            required_evidence_types,
            profile_refs,
        })
    }

    pub fn bind_snapshots(
        &self,
        snapshots: &[VerifierSnapshot],
    ) -> Result<BoundVerificationPlan, VerificationError> {
        let mut by_id = BTreeMap::new();
        for snapshot in snapshots {
            if by_id
                .insert(
                    snapshot.verifier_id.clone(),
                    snapshot.snapshot_digest.clone(),
                )
                .is_some()
            {
                return Err(VerificationError::DuplicateVerifierSnapshot(
                    snapshot.verifier_id.clone(),
                ));
            }
        }
        for required in &self.required_verifiers {
            if !by_id.contains_key(required) {
                return Err(VerificationError::MissingVerifierSnapshot(required.clone()));
            }
        }

        let mut hasher = Sha256::new();
        hash_field(&mut hasher, b"AER_VERIFICATION_COMPOSITION_V1");
        for verifier_id in &self.required_verifiers {
            hash_field(&mut hasher, verifier_id.as_bytes());
            let digest = by_id
                .get(verifier_id)
                .expect("required verifier snapshot was checked above");
            hash_field(&mut hasher, digest.as_bytes());
        }
        for evidence_type in &self.required_evidence_types {
            hash_field(&mut hasher, evidence_type.as_str().as_bytes());
        }
        for profile in &self.profile_refs {
            hash_field(&mut hasher, profile.as_bytes());
        }
        Ok(BoundVerificationPlan {
            required_verifiers: self.required_verifiers.clone(),
            required_evidence_types: self.required_evidence_types.clone(),
            verifier_snapshots: by_id,
            composition_snapshot: lowercase_hex(hasher.finalize().as_ref()),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundVerificationPlan {
    pub required_verifiers: BTreeSet<String>,
    pub required_evidence_types: BTreeSet<EvidenceType>,
    pub verifier_snapshots: BTreeMap<String, String>,
    pub composition_snapshot: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceCacheKey {
    pub repo_snapshot: String,
    pub environment_fingerprint: String,
    pub verifier_snapshot: String,
    pub input_artifact_hashes: Vec<String>,
}

impl EvidenceCacheKey {
    pub fn from_record(record: &Value) -> Result<Self, VerificationError> {
        let repo_snapshot = required_str(record, "repo_snapshot")?.to_owned();
        let environment_fingerprint = required_str(record, "environment_fingerprint")?.to_owned();
        let verifier_snapshot = record
            .get("integrity")
            .and_then(|value| value.get("verifier_snapshot"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(VerificationError::EvidenceMissingIntegrity)?
            .to_owned();
        let mut input_artifact_hashes = record
            .get("input_artifact_hashes")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        input_artifact_hashes.sort();
        Ok(Self {
            repo_snapshot,
            environment_fingerprint,
            verifier_snapshot,
            input_artifact_hashes,
        })
    }

    #[must_use]
    pub fn reusable_for(
        &self,
        repo_snapshot: &str,
        environment_fingerprint: &str,
        verifier_snapshot: &str,
        input_artifact_hashes: &[String],
    ) -> bool {
        let mut current_inputs = input_artifact_hashes.to_vec();
        current_inputs.sort();
        self.repo_snapshot == repo_snapshot
            && self.environment_fingerprint == environment_fingerprint
            && self.verifier_snapshot == verifier_snapshot
            && self.input_artifact_hashes == current_inputs
    }
}

pub struct VerifierRunRequest<'a> {
    pub repo_snapshot: &'a str,
    pub requirement_refs: &'a [String],
    pub observed_at: &'a str,
    pub input_artifact_hashes: &'a [String],
    pub workspace_root: &'a Path,
    pub environment: &'a EnvironmentFingerprint,
    pub definition: &'a VerifierDefinition,
    pub trusted_snapshot: &'a VerifierSnapshot,
}

pub fn run_verifier(request: VerifierRunRequest<'_>) -> Result<Value, VerificationError> {
    if request.repo_snapshot.trim().is_empty()
        || request.requirement_refs.is_empty()
        || request.observed_at.trim().is_empty()
        || request.environment.digest.trim().is_empty()
    {
        return Err(VerificationError::InvalidEvidenceRequest);
    }
    ensure_unique_nonempty(request.requirement_refs)?;
    for hash in request.input_artifact_hashes {
        validate_sha256(hash)?;
    }
    request
        .trusted_snapshot
        .assert_candidate_unchanged(request.definition, request.workspace_root)?;

    let policy = ExecutionPolicy::trusted_workspace(
        request.workspace_root,
        request.definition.timeout,
        request.definition.max_capture_bytes,
    )
    .map_err(|error| VerificationError::Execution(error.to_string()))?
    .require_strong_isolation(request.definition.require_strong_isolation);
    let spec = CommandSpec::new(
        request.definition.executable.clone(),
        request.workspace_root,
        SideEffectClass::ProcessExecution,
    )
    .args(request.definition.args.clone());
    let result = LocalProcessExecutor
        .execute(&policy, spec)
        .map_err(|error| VerificationError::Execution(error.to_string()))?;
    let bound = CommandExecutionEvidence::bind(request.repo_snapshot, request.environment, &result)
        .map_err(|error| VerificationError::EvidenceBinding(error.to_string()))?;

    let outcome = if result.timed_out {
        "error"
    } else if result.success {
        "pass"
    } else {
        "fail"
    };
    let security_profile = match bound.security_profile {
        SecurityProfile::DirectHostProcess => "direct_host_process",
    };
    let duration_ms = u64::try_from(bound.duration_ms).unwrap_or(u64::MAX);
    let evidence = json!({
        "schema_version": 1,
        "evidence_id": format!("EVD-{}", Ulid::generate()),
        "type": request.definition.evidence_type.as_str(),
        "requirement_refs": request.requirement_refs,
        "repo_snapshot": request.repo_snapshot,
        "command_or_tool": {
            "verifier_id": request.definition.verifier_id,
            "verifier_version": request.definition.version,
            "argv": bound.argv,
            "cwd": bound.cwd.to_string_lossy(),
        },
        "environment_fingerprint": bound.environment_digest,
        "input_artifact_hashes": request.input_artifact_hashes,
        "output_artifact_hashes": [bound.stdout_sha256, bound.stderr_sha256],
        "result": outcome,
        "measurements": {
            "duration_ms": duration_ms,
            "exit_code": bound.exit_code,
            "stdout_bytes": bound.stdout_bytes,
            "stderr_bytes": bound.stderr_bytes,
            "timed_out": bound.timed_out,
        },
        "timestamp": request.observed_at,
        "integrity": {
            "verifier_snapshot": request.trusted_snapshot.snapshot_digest,
            "generator_could_modify_verifier": false,
            "command_evidence_digest": bound.evidence_digest,
            "security_profile": security_profile,
            "verifier_layer": request.definition.layer.as_str(),
        }
    });
    validate_contract(CoreContract::EvidenceRecord, &evidence)?;
    Ok(evidence)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImplementationLocation {
    pub path: String,
    pub symbol: Option<String>,
}

impl ImplementationLocation {
    pub fn new(path: impl Into<String>, symbol: Option<String>) -> Result<Self, VerificationError> {
        let path = path.into();
        if path.trim().is_empty() {
            return Err(VerificationError::InvalidImplementationLocation(path));
        }
        validate_relative_path(Path::new(&path))?;
        if symbol.as_deref().is_some_and(str::is_empty) {
            return Err(VerificationError::InvalidImplementationLocation(path));
        }
        Ok(Self { path, symbol })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequirementProofInput {
    pub requirement_id: String,
    pub implementation: Vec<ImplementationLocation>,
}

pub fn build_proof_manifest(
    engineering_ir: &Value,
    task: &Value,
    implementation_root: impl AsRef<Path>,
    mappings: &[RequirementProofInput],
    evidence: &[Value],
    plan: &BoundVerificationPlan,
) -> Result<Value, VerificationError> {
    let implementation_root = canonical_directory(implementation_root.as_ref())?;
    let task_id = required_str(task, "task_id")?;
    let repo_snapshot = required_str(task, "repo_snapshot")?;
    let spec_version = task
        .get("spec_version")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or(VerificationError::InvalidTaskForProof)?;
    let task_requirements = string_set(task, "requirement_refs")?;
    if task_requirements.is_empty() {
        return Err(VerificationError::InvalidTaskForProof);
    }

    let mapping_ids = mappings
        .iter()
        .map(|mapping| mapping.requirement_id.clone())
        .collect::<BTreeSet<_>>();
    if mapping_ids.len() != mappings.len() || mapping_ids != task_requirements {
        return Err(VerificationError::RequirementCoverageMismatch);
    }
    for mapping in mappings {
        if mapping.implementation.is_empty() {
            return Err(VerificationError::MissingImplementation(
                mapping.requirement_id.clone(),
            ));
        }
        for location in &mapping.implementation {
            let path = implementation_root.join(&location.path);
            if !path.exists() {
                return Err(VerificationError::ImplementationPathMissing(
                    location.path.clone(),
                ));
            }
        }
    }

    let mut passing_verifiers = BTreeSet::new();
    let mut passing_types = BTreeSet::new();
    let mut passing_by_requirement = BTreeMap::<String, Vec<String>>::new();
    for record in evidence {
        validate_contract(CoreContract::EvidenceRecord, record)?;
        if required_str(record, "repo_snapshot")? != repo_snapshot {
            return Err(VerificationError::StaleEvidenceRepoSnapshot);
        }
        if record.get("result").and_then(Value::as_str) != Some("pass") {
            continue;
        }
        let environment = required_str(record, "environment_fingerprint")?;
        if environment.is_empty() {
            return Err(VerificationError::EvidenceMissingEnvironment);
        }
        let integrity = record
            .get("integrity")
            .and_then(Value::as_object)
            .ok_or(VerificationError::EvidenceMissingIntegrity)?;
        if integrity
            .get("generator_could_modify_verifier")
            .and_then(Value::as_bool)
            != Some(false)
        {
            return Err(VerificationError::GeneratorControlledEvidence);
        }
        let verifier_snapshot = integrity
            .get("verifier_snapshot")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(VerificationError::EvidenceMissingIntegrity)?;
        let verifier_id = record
            .get("command_or_tool")
            .and_then(|value| value.get("verifier_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(VerificationError::EvidenceMissingVerifierIdentity)?;
        if let Some(expected_snapshot) = plan.verifier_snapshots.get(verifier_id) {
            if expected_snapshot != verifier_snapshot {
                return Err(VerificationError::VerifierSnapshotMismatch {
                    verifier_id: verifier_id.to_owned(),
                });
            }
            passing_verifiers.insert(verifier_id.to_owned());
        }
        if let Some(value) = record.get("type").and_then(Value::as_str)
            && let Some(evidence_type) = evidence_type_from_str(value)
        {
            passing_types.insert(evidence_type);
        }
        let evidence_id = required_str(record, "evidence_id")?.to_owned();
        for requirement in string_set(record, "requirement_refs")? {
            passing_by_requirement
                .entry(requirement)
                .or_default()
                .push(evidence_id.clone());
        }
    }

    for verifier_id in &plan.required_verifiers {
        if !passing_verifiers.contains(verifier_id) {
            return Err(VerificationError::RequiredVerifierDidNotPass(
                verifier_id.clone(),
            ));
        }
    }
    for evidence_type in &plan.required_evidence_types {
        if !passing_types.contains(evidence_type) {
            return Err(VerificationError::RequiredEvidenceTypeMissing(
                evidence_type.as_str().to_owned(),
            ));
        }
    }

    let requirements = mappings
        .iter()
        .map(|mapping| {
            let evidence_ids = passing_by_requirement
                .get(&mapping.requirement_id)
                .filter(|ids| !ids.is_empty())
                .ok_or_else(|| {
                    VerificationError::MissingPassingEvidence(mapping.requirement_id.clone())
                })?;
            let implementation = mapping
                .implementation
                .iter()
                .map(|location| match &location.symbol {
                    Some(symbol) => json!({"path": location.path, "symbol": symbol}),
                    None => json!({"path": location.path}),
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "id": mapping.requirement_id,
                "implementation": implementation,
                "evidence": evidence_ids,
                "verdict": "pass"
            }))
        })
        .collect::<Result<Vec<_>, VerificationError>>()?;

    let proof = json!({
        "schema_version": 1,
        "task_id": task_id,
        "repo_snapshot": repo_snapshot,
        "spec_version": spec_version,
        "requirements": requirements,
        "integrity": {
            "verifier_snapshot": plan.composition_snapshot,
            "generator_could_modify_verifier": false
        },
        "overall_verdict": "pass"
    });
    validate_contract(CoreContract::ProofManifest, &proof)?;
    let issues = validate_semantic_bundle(&SemanticBundle {
        engineering_ir: engineering_ir.clone(),
        tasks: vec![task.clone()],
        evidence: evidence.to_vec(),
        proof_manifests: vec![proof.clone()],
    });
    if !issues.is_empty() {
        return Err(VerificationError::SemanticProofInvalid(
            issues
                .into_iter()
                .map(|issue| format!("{} {}: {}", issue.code, issue.path, issue.message))
                .collect(),
        ));
    }
    Ok(proof)
}

#[derive(Debug)]
pub struct PersistedVerification {
    pub evidence_artifacts: Vec<ObjectHash>,
    pub evidence_events: Vec<StoredEvent>,
    pub proof_artifact: ObjectHash,
    pub verdict_event: StoredEvent,
    pub accepted_event: StoredEvent,
}

pub fn persist_accepted_verification(
    state: &mut DurableState,
    project_id: &str,
    run_id: Option<&str>,
    task_id: &str,
    evidence: &[Value],
    proof: &Value,
) -> Result<PersistedVerification, VerificationError> {
    if project_id.trim().is_empty() || task_id.trim().is_empty() {
        return Err(VerificationError::InvalidPersistenceIdentity);
    }
    validate_contract(CoreContract::ProofManifest, proof)?;
    if proof.get("task_id").and_then(Value::as_str) != Some(task_id)
        || proof.get("overall_verdict").and_then(Value::as_str) != Some("pass")
        || proof
            .get("integrity")
            .and_then(|value| value.get("generator_could_modify_verifier"))
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(VerificationError::ProofNotAcceptable);
    }
    TaskState::Verifying
        .transition(
            TaskState::Accepted,
            TaskTransitionContext {
                proof_accepted: true,
                cancellation_finalized: false,
            },
        )
        .map_err(|error| VerificationError::TaskTransition(error.to_string()))?;

    let mut metadata = ObjectMetadata::new(Sensitivity::Internal, "verification-proof");
    metadata.pinned = true;
    let mut evidence_artifacts = Vec::new();
    let mut evidence_events = Vec::new();
    for record in evidence {
        validate_contract(CoreContract::EvidenceRecord, record)?;
        let bytes = serde_json::to_vec(record)
            .map_err(|error| VerificationError::Serialization(error.to_string()))?;
        let hash = state
            .put_object(project_id, &bytes, &metadata)
            .map_err(|error| VerificationError::Storage(error.to_string()))?;
        let mut event = NewEvent::new(project_id, "evidence.created");
        event.run_id = run_id.map(str::to_owned);
        event.task_id = Some(task_id.to_owned());
        event.payload = EventPayload::Artifact(hash.clone());
        event.causation = Some(Causation::External(format!("verification:{task_id}")));
        event.correlation_id = Some(task_id.to_owned());
        let stored = state
            .append_event(event)
            .map_err(|error| VerificationError::Storage(error.to_string()))?;
        evidence_artifacts.push(hash);
        evidence_events.push(stored);
    }

    let proof_bytes = serde_json::to_vec(proof)
        .map_err(|error| VerificationError::Serialization(error.to_string()))?;
    let proof_artifact = state
        .put_object(project_id, &proof_bytes, &metadata)
        .map_err(|error| VerificationError::Storage(error.to_string()))?;
    let mut verdict = NewEvent::new(project_id, "verification.verdict");
    verdict.run_id = run_id.map(str::to_owned);
    verdict.task_id = Some(task_id.to_owned());
    verdict.payload = EventPayload::Artifact(proof_artifact.clone());
    verdict.causation = evidence_events
        .last()
        .map(|event| Causation::Event(event.event_id.clone()))
        .or_else(|| Some(Causation::External(format!("verification:{task_id}"))));
    verdict.correlation_id = Some(task_id.to_owned());
    let verdict_event = state
        .append_event(verdict)
        .map_err(|error| VerificationError::Storage(error.to_string()))?;

    let mut accepted = NewEvent::new(project_id, "task.accepted");
    accepted.run_id = run_id.map(str::to_owned);
    accepted.task_id = Some(task_id.to_owned());
    accepted.payload = EventPayload::Inline(json!({
        "proof_artifact": proof_artifact.as_str(),
        "verification_event_id": verdict_event.event_id,
    }));
    accepted.causation = Some(Causation::Event(verdict_event.event_id.clone()));
    accepted.correlation_id = Some(task_id.to_owned());
    let accepted_event = state
        .append_event(accepted)
        .map_err(|error| VerificationError::Storage(error.to_string()))?;

    Ok(PersistedVerification {
        evidence_artifacts,
        evidence_events,
        proof_artifact,
        verdict_event,
        accepted_event,
    })
}

fn validate_contract(contract: CoreContract, value: &Value) -> Result<(), VerificationError> {
    let registry = EmbeddedContractRegistry::load()
        .map_err(|error| VerificationError::Contract(error.to_string()))?;
    registry
        .validate_current(contract, value)
        .map_err(|error| VerificationError::Contract(error.to_string()))
}

fn collect_protected_assets(
    root: &Path,
    protected_paths: &[PathBuf],
) -> Result<BTreeMap<String, String>, VerificationError> {
    let mut assets = BTreeMap::new();
    for relative in protected_paths {
        let full = root.join(relative);
        collect_asset(root, &full, &mut assets)?;
    }
    Ok(assets)
}

fn collect_asset(
    root: &Path,
    path: &Path,
    assets: &mut BTreeMap<String, String>,
) -> Result<(), VerificationError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| VerificationError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(VerificationError::UnsupportedVerifierAsset(
            path.to_path_buf(),
        ));
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| VerificationError::VerifierPathEscapesRoot(path.to_path_buf()))?;
    if metadata.is_dir() {
        let directory_key = format!("{}/", portable_path(relative)?);
        assets.insert(directory_key, "directory".to_owned());
        let mut children = fs::read_dir(path)
            .map_err(|source| VerificationError::Io {
                path: path.to_path_buf(),
                source,
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| VerificationError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            collect_asset(root, &child.path(), assets)?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(VerificationError::UnsupportedVerifierAsset(
            path.to_path_buf(),
        ));
    }
    let bytes = fs::read(path).map_err(|source| VerificationError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    assets.insert(portable_path(relative)?, sha256(&bytes));
    Ok(())
}

fn snapshot_digest(definition_digest: &str, assets: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, b"AER_VERIFIER_SNAPSHOT_V1");
    hash_field(&mut hasher, definition_digest.as_bytes());
    for (path, digest) in assets {
        hash_field(&mut hasher, path.as_bytes());
        hash_field(&mut hasher, digest.as_bytes());
    }
    lowercase_hex(hasher.finalize().as_ref())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, VerificationError> {
    let canonical = path
        .canonicalize()
        .map_err(|source| VerificationError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if !canonical.is_dir() {
        return Err(VerificationError::ExpectedDirectory(canonical));
    }
    Ok(canonical)
}

fn validate_relative_path(path: &Path) -> Result<(), VerificationError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(VerificationError::InvalidRelativePath(path.to_path_buf()));
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(VerificationError::InvalidRelativePath(path.to_path_buf()));
        }
    }
    Ok(())
}

fn portable_path(path: &Path) -> Result<String, VerificationError> {
    validate_relative_path(path)?;
    let mut output = String::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return Err(VerificationError::InvalidRelativePath(path.to_path_buf()));
        };
        let value = value
            .to_str()
            .ok_or_else(|| VerificationError::NonUtf8VerifierPath(path.to_path_buf()))?;
        if !output.is_empty() {
            output.push('/');
        }
        output.push_str(value);
    }
    Ok(output)
}

fn string_set(root: &Value, key: &'static str) -> Result<BTreeSet<String>, VerificationError> {
    let values = root
        .get(key)
        .and_then(Value::as_array)
        .ok_or(VerificationError::InvalidStringArray(key))?;
    let mut output = BTreeSet::new();
    for value in values {
        let value = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(VerificationError::InvalidStringArray(key))?;
        if !output.insert(value.to_owned()) {
            return Err(VerificationError::DuplicateStringValue {
                field: key,
                value: value.to_owned(),
            });
        }
    }
    Ok(output)
}

fn ensure_unique_nonempty(values: &[String]) -> Result<(), VerificationError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() || !seen.insert(value) {
            return Err(VerificationError::InvalidEvidenceRequest);
        }
    }
    Ok(())
}

fn required_str<'a>(root: &'a Value, key: &'static str) -> Result<&'a str, VerificationError> {
    root.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(VerificationError::MissingStringField(key))
}

fn validate_sha256(value: &str) -> Result<(), VerificationError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(VerificationError::InvalidArtifactHash(value.to_owned()))
    }
}

fn evidence_type_from_str(value: &str) -> Option<EvidenceType> {
    match value {
        "command" => Some(EvidenceType::Command),
        "test" => Some(EvidenceType::Test),
        "static_analysis" => Some(EvidenceType::StaticAnalysis),
        "security" => Some(EvidenceType::Security),
        "performance" => Some(EvidenceType::Performance),
        "architecture" => Some(EvidenceType::Architecture),
        "semantic_review" => Some(EvidenceType::SemanticReview),
        "runtime_trace" => Some(EvidenceType::RuntimeTrace),
        "manual" => Some(EvidenceType::Manual),
        _ => None,
    }
}

fn sha256(bytes: &[u8]) -> String {
    lowercase_hex(Sha256::digest(bytes).as_ref())
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

#[derive(Debug)]
pub enum VerificationError {
    InvalidVerifierDefinition,
    DuplicateProtectedPath(PathBuf),
    InvalidDomainProfile,
    InvalidVerificationPlan,
    DuplicateVerifierSnapshot(String),
    MissingVerifierSnapshot(String),
    VerifierDefinitionChanged {
        verifier_id: String,
    },
    VerifierIntegrityViolation {
        verifier_id: String,
        expected: String,
        actual: String,
    },
    InvalidEvidenceRequest,
    EvidenceBinding(String),
    Execution(String),
    Contract(String),
    InvalidTaskForProof,
    RequirementCoverageMismatch,
    MissingImplementation(String),
    ImplementationPathMissing(String),
    InvalidImplementationLocation(String),
    StaleEvidenceRepoSnapshot,
    EvidenceMissingEnvironment,
    EvidenceMissingIntegrity,
    EvidenceMissingVerifierIdentity,
    GeneratorControlledEvidence,
    VerifierSnapshotMismatch {
        verifier_id: String,
    },
    RequiredVerifierDidNotPass(String),
    RequiredEvidenceTypeMissing(String),
    MissingPassingEvidence(String),
    SemanticProofInvalid(Vec<String>),
    ProofNotAcceptable,
    InvalidPersistenceIdentity,
    TaskTransition(String),
    Storage(String),
    Serialization(String),
    InvalidRelativePath(PathBuf),
    NonUtf8VerifierPath(PathBuf),
    VerifierPathEscapesRoot(PathBuf),
    UnsupportedVerifierAsset(PathBuf),
    ExpectedDirectory(PathBuf),
    MissingStringField(&'static str),
    InvalidStringArray(&'static str),
    DuplicateStringValue {
        field: &'static str,
        value: String,
    },
    InvalidArtifactHash(String),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for VerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVerifierDefinition => formatter.write_str("invalid verifier definition"),
            Self::DuplicateProtectedPath(path) => {
                write!(
                    formatter,
                    "duplicate protected verifier path: {}",
                    path.display()
                )
            }
            Self::InvalidDomainProfile => {
                formatter.write_str("invalid domain verification profile")
            }
            Self::InvalidVerificationPlan => formatter.write_str("invalid verification plan"),
            Self::DuplicateVerifierSnapshot(id) => {
                write!(formatter, "duplicate verifier snapshot: {id}")
            }
            Self::MissingVerifierSnapshot(id) => {
                write!(formatter, "missing required verifier snapshot: {id}")
            }
            Self::VerifierDefinitionChanged { verifier_id } => {
                write!(
                    formatter,
                    "verifier definition changed after authority snapshot: {verifier_id}"
                )
            }
            Self::VerifierIntegrityViolation {
                verifier_id,
                expected,
                actual,
            } => write!(
                formatter,
                "verifier integrity violation for {verifier_id}: expected {expected}, actual {actual}"
            ),
            Self::InvalidEvidenceRequest => {
                formatter.write_str("invalid verification evidence request")
            }
            Self::EvidenceBinding(message) => {
                write!(formatter, "evidence binding failed: {message}")
            }
            Self::Execution(message) => write!(formatter, "verifier execution failed: {message}"),
            Self::Contract(message) => write!(formatter, "verification contract failed: {message}"),
            Self::InvalidTaskForProof => {
                formatter.write_str("task cannot produce a proof manifest")
            }
            Self::RequirementCoverageMismatch => {
                formatter.write_str("proof mapping does not exactly cover task requirements")
            }
            Self::MissingImplementation(requirement) => write!(
                formatter,
                "requirement {requirement} has no implementation location"
            ),
            Self::ImplementationPathMissing(path) => write!(
                formatter,
                "proof implementation path does not exist: {path}"
            ),
            Self::InvalidImplementationLocation(path) => {
                write!(formatter, "invalid proof implementation location: {path}")
            }
            Self::StaleEvidenceRepoSnapshot => {
                formatter.write_str("evidence belongs to a different repository snapshot")
            }
            Self::EvidenceMissingEnvironment => {
                formatter.write_str("passing evidence has no environment fingerprint")
            }
            Self::EvidenceMissingIntegrity => {
                formatter.write_str("passing evidence has incomplete verifier integrity metadata")
            }
            Self::EvidenceMissingVerifierIdentity => {
                formatter.write_str("passing evidence has no verifier identity")
            }
            Self::GeneratorControlledEvidence => {
                formatter.write_str("generator-controlled evidence cannot support accepted proof")
            }
            Self::VerifierSnapshotMismatch { verifier_id } => write!(
                formatter,
                "evidence verifier snapshot is stale or mismatched: {verifier_id}"
            ),
            Self::RequiredVerifierDidNotPass(id) => write!(
                formatter,
                "required verifier did not produce passing evidence: {id}"
            ),
            Self::RequiredEvidenceTypeMissing(kind) => {
                write!(formatter, "required evidence type is missing: {kind}")
            }
            Self::MissingPassingEvidence(requirement) => write!(
                formatter,
                "requirement {requirement} has no passing evidence"
            ),
            Self::SemanticProofInvalid(issues) => write!(
                formatter,
                "proof semantic validation failed: {}",
                issues.join("; ")
            ),
            Self::ProofNotAcceptable => {
                formatter.write_str("proof is not eligible for accepted task state")
            }
            Self::InvalidPersistenceIdentity => {
                formatter.write_str("verification persistence requires project and task identity")
            }
            Self::TaskTransition(message) => {
                write!(formatter, "task acceptance transition failed: {message}")
            }
            Self::Storage(message) => {
                write!(formatter, "verification persistence failed: {message}")
            }
            Self::Serialization(message) => {
                write!(formatter, "verification serialization failed: {message}")
            }
            Self::InvalidRelativePath(path) => write!(
                formatter,
                "path must be safe and relative: {}",
                path.display()
            ),
            Self::NonUtf8VerifierPath(path) => write!(
                formatter,
                "verifier asset path is not UTF-8: {}",
                path.display()
            ),
            Self::VerifierPathEscapesRoot(path) => write!(
                formatter,
                "verifier asset escaped authority root: {}",
                path.display()
            ),
            Self::UnsupportedVerifierAsset(path) => write!(
                formatter,
                "unsupported verifier asset type: {}",
                path.display()
            ),
            Self::ExpectedDirectory(path) => {
                write!(formatter, "expected directory: {}", path.display())
            }
            Self::MissingStringField(field) => {
                write!(formatter, "missing required string field: {field}")
            }
            Self::InvalidStringArray(field) => write!(formatter, "invalid string array: {field}"),
            Self::DuplicateStringValue { field, value } => {
                write!(formatter, "duplicate value in {field}: {value}")
            }
            Self::InvalidArtifactHash(value) => {
                write!(formatter, "invalid SHA-256 artifact hash: {value}")
            }
            Self::Io { path, source } => {
                write!(formatter, "I/O error at {}: {source}", path.display())
            }
        }
    }
}

impl Error for VerificationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, time::Duration};

    use aer_environment::EnvironmentFingerprint;
    use aer_storage::DurableState;
    use serde_json::{Value, json};
    use ulid::Ulid;

    use super::{
        DomainProfile, EvidenceCacheKey, EvidenceType, ImplementationLocation,
        RequirementProofInput, VerificationError, VerificationLayer, VerificationPlan,
        VerifierDefinition, VerifierRunRequest, VerifierSnapshot, build_proof_manifest,
        persist_accepted_verification, run_verifier,
    };

    struct TestDirectory {
        path: std::path::PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("aer-verify-{label}-{}", Ulid::generate()));
            fs::create_dir_all(&path).expect("create test directory");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn verifier(
        root: &Path,
        require_strong_isolation: bool,
    ) -> (VerifierDefinition, VerifierSnapshot) {
        fs::create_dir_all(root.join("tests")).expect("tests directory");
        fs::write(root.join("tests/integrity.txt"), b"immutable\n").expect("verifier asset");
        let definition = VerifierDefinition::new(
            "rust-core-tests",
            1,
            VerificationLayer::LocalBehavior,
            EvidenceType::Test,
            "rustc",
            vec!["--version".to_owned()],
            vec!["tests".into()],
            Duration::from_secs(10),
            64 * 1024,
            require_strong_isolation,
        )
        .expect("definition");
        let snapshot = VerifierSnapshot::capture(&definition, root).expect("snapshot");
        (definition, snapshot)
    }

    fn environment(digest: &str) -> EnvironmentFingerprint {
        EnvironmentFingerprint {
            os: "test".to_owned(),
            architecture: "x86_64".to_owned(),
            family: "test".to_owned(),
            os_version: None,
            shell: None,
            locale: None,
            timezone: None,
            tools: Vec::new(),
            lockfiles: Vec::new(),
            environment_signals: Vec::new(),
            digest: digest.to_owned(),
        }
    }

    fn passing_evidence(repo_snapshot: &str, requirement: &str, verifier_snapshot: &str) -> Value {
        json!({
            "schema_version": 1,
            "evidence_id": "EVD-proof",
            "type": "test",
            "requirement_refs": [requirement],
            "repo_snapshot": repo_snapshot,
            "command_or_tool": {
                "verifier_id": "rust-core-tests",
                "verifier_version": 1
            },
            "environment_fingerprint": "env-a",
            "input_artifact_hashes": ["a".repeat(64)],
            "output_artifact_hashes": ["b".repeat(64)],
            "result": "pass",
            "timestamp": "2026-08-16T09:00:00Z",
            "integrity": {
                "verifier_snapshot": verifier_snapshot,
                "generator_could_modify_verifier": false
            }
        })
    }

    fn ir_and_task(repo_snapshot: &str) -> (Value, Value) {
        let ir = json!({
            "functional_requirements": [{"id": "REQ-1", "dependencies": []}],
            "acceptance_criteria": [],
            "invariants": []
        });
        let task = json!({
            "task_id": "TASK-1",
            "requirement_refs": ["REQ-1"],
            "acceptance_refs": [],
            "invariant_refs": [],
            "dependencies": [],
            "spec_version": 1,
            "repo_snapshot": repo_snapshot,
            "state": "verifying"
        });
        (ir, task)
    }

    #[test]
    fn immutable_verifier_detects_deliberate_test_tampering() {
        let directory = TestDirectory::new("tamper");
        let (definition, snapshot) = verifier(&directory.path, false);
        fs::write(directory.path.join("tests/integrity.txt"), b"weakened\n").expect("tamper");
        let error = snapshot
            .assert_candidate_unchanged(&definition, &directory.path)
            .expect_err("tampering must fail closed");
        assert!(matches!(
            error,
            VerificationError::VerifierIntegrityViolation { .. }
        ));
    }

    #[test]
    fn domain_profiles_can_only_add_to_mandatory_verification() {
        let profile = DomainProfile::new(
            "cli-rust",
            1,
            ["cli-behavior".to_owned()],
            [EvidenceType::Test],
        )
        .expect("profile");
        let plan = VerificationPlan::compose(
            ["org-security".to_owned()],
            [EvidenceType::Security],
            &[profile],
        )
        .expect("plan");
        assert!(plan.required_verifiers.contains("org-security"));
        assert!(plan.required_verifiers.contains("cli-behavior"));
        assert!(
            plan.required_evidence_types
                .contains(&EvidenceType::Security)
        );
        assert!(plan.required_evidence_types.contains(&EvidenceType::Test));
    }

    #[test]
    fn environment_repo_verifier_and_inputs_are_hard_cache_boundaries() {
        let record = json!({
            "repo_snapshot": "repo-a",
            "environment_fingerprint": "env-a",
            "input_artifact_hashes": ["b", "a"],
            "integrity": {"verifier_snapshot": "verifier-a"}
        });
        let key = EvidenceCacheKey::from_record(&record).expect("cache key");
        let inputs = vec!["a".to_owned(), "b".to_owned()];
        assert!(key.reusable_for("repo-a", "env-a", "verifier-a", &inputs));
        assert!(!key.reusable_for("repo-b", "env-a", "verifier-a", &inputs));
        assert!(!key.reusable_for("repo-a", "env-b", "verifier-a", &inputs));
        assert!(!key.reusable_for("repo-a", "env-a", "verifier-b", &inputs));
        assert!(!key.reusable_for("repo-a", "env-a", "verifier-a", &["c".to_owned()]));
    }

    #[test]
    fn strong_isolation_requirement_fails_closed_before_verifier_spawn() {
        let directory = TestDirectory::new("strong-isolation");
        let (definition, snapshot) = verifier(&directory.path, true);
        let requirements = vec!["REQ-1".to_owned()];
        let error = run_verifier(VerifierRunRequest {
            repo_snapshot: "repo-a",
            requirement_refs: &requirements,
            observed_at: "2026-08-16T09:00:00Z",
            input_artifact_hashes: &["a".repeat(64)],
            workspace_root: &directory.path,
            environment: &environment("env-a"),
            definition: &definition,
            trusted_snapshot: &snapshot,
        })
        .expect_err("direct executor must not fake strong isolation");
        assert!(matches!(error, VerificationError::Execution(_)));
        assert!(error.to_string().contains("strong isolation"));
    }

    #[test]
    fn command_evidence_is_bound_to_repo_environment_and_verifier() {
        let directory = TestDirectory::new("evidence");
        let (definition, snapshot) = verifier(&directory.path, false);
        let requirements = vec!["REQ-1".to_owned()];
        let input_hashes = vec!["a".repeat(64)];
        let environment = environment("env-a");
        let evidence = run_verifier(VerifierRunRequest {
            repo_snapshot: "repo-a",
            requirement_refs: &requirements,
            observed_at: "2026-08-16T09:00:00Z",
            input_artifact_hashes: &input_hashes,
            workspace_root: &directory.path,
            environment: &environment,
            definition: &definition,
            trusted_snapshot: &snapshot,
        })
        .expect("evidence");
        assert_eq!(evidence["result"], "pass");
        assert_eq!(evidence["repo_snapshot"], "repo-a");
        assert_eq!(evidence["environment_fingerprint"], "env-a");
        assert_eq!(
            evidence["integrity"]["verifier_snapshot"],
            snapshot.snapshot_digest
        );
        assert_eq!(
            evidence["integrity"]["generator_could_modify_verifier"],
            false
        );
    }

    #[test]
    fn proof_requires_exact_requirement_code_and_passing_evidence_chain() {
        let directory = TestDirectory::new("proof");
        fs::create_dir_all(directory.path.join("src")).expect("src");
        fs::write(
            directory.path.join("src/lib.rs"),
            b"pub fn value() -> u8 { 1 }\n",
        )
        .expect("implementation");
        let (definition, snapshot) = verifier(&directory.path, false);
        let plan =
            VerificationPlan::compose([definition.verifier_id.clone()], [EvidenceType::Test], &[])
                .expect("plan")
                .bind_snapshots(std::slice::from_ref(&snapshot))
                .expect("bound plan");
        let (ir, task) = ir_and_task("repo-a");
        let evidence = vec![passing_evidence(
            "repo-a",
            "REQ-1",
            &snapshot.snapshot_digest,
        )];
        let mappings = vec![RequirementProofInput {
            requirement_id: "REQ-1".to_owned(),
            implementation: vec![
                ImplementationLocation::new("src/lib.rs", Some("value".to_owned()))
                    .expect("location"),
            ],
        }];
        let proof = build_proof_manifest(&ir, &task, &directory.path, &mappings, &evidence, &plan)
            .expect("proof");
        assert_eq!(proof["overall_verdict"], "pass");
        assert_eq!(proof["requirements"][0]["id"], "REQ-1");
        assert_eq!(proof["requirements"][0]["evidence"][0], "EVD-proof");
    }

    #[test]
    fn stale_repository_evidence_cannot_support_current_proof() {
        let directory = TestDirectory::new("stale-proof");
        fs::create_dir_all(directory.path.join("src")).expect("src");
        fs::write(directory.path.join("src/lib.rs"), b"pub fn value() {}\n")
            .expect("implementation");
        let (definition, snapshot) = verifier(&directory.path, false);
        let plan = VerificationPlan::compose([definition.verifier_id.clone()], [], &[])
            .expect("plan")
            .bind_snapshots(std::slice::from_ref(&snapshot))
            .expect("bound plan");
        let (ir, task) = ir_and_task("repo-current");
        let evidence = vec![passing_evidence(
            "repo-old",
            "REQ-1",
            &snapshot.snapshot_digest,
        )];
        let mappings = vec![RequirementProofInput {
            requirement_id: "REQ-1".to_owned(),
            implementation: vec![
                ImplementationLocation::new("src/lib.rs", None).expect("location"),
            ],
        }];
        let error = build_proof_manifest(&ir, &task, &directory.path, &mappings, &evidence, &plan)
            .expect_err("stale evidence must fail");
        assert!(matches!(
            error,
            VerificationError::StaleEvidenceRepoSnapshot
        ));
    }

    #[test]
    fn accepted_task_persists_evidence_then_proof_verdict_then_acceptance() {
        let directory = TestDirectory::new("persist");
        fs::create_dir_all(directory.path.join("src")).expect("src");
        fs::write(directory.path.join("src/lib.rs"), b"pub fn value() {}\n")
            .expect("implementation");
        let (definition, snapshot) = verifier(&directory.path, false);
        let plan =
            VerificationPlan::compose([definition.verifier_id.clone()], [EvidenceType::Test], &[])
                .expect("plan")
                .bind_snapshots(std::slice::from_ref(&snapshot))
                .expect("bound plan");
        let (ir, task) = ir_and_task("repo-a");
        let evidence = vec![passing_evidence(
            "repo-a",
            "REQ-1",
            &snapshot.snapshot_digest,
        )];
        let mappings = vec![RequirementProofInput {
            requirement_id: "REQ-1".to_owned(),
            implementation: vec![
                ImplementationLocation::new("src/lib.rs", None).expect("location"),
            ],
        }];
        let proof = build_proof_manifest(&ir, &task, &directory.path, &mappings, &evidence, &plan)
            .expect("proof");
        let mut state = DurableState::open(directory.path.join("durable")).expect("store");
        let persisted = persist_accepted_verification(
            &mut state,
            "project-a",
            Some("run-a"),
            "TASK-1",
            &evidence,
            &proof,
        )
        .expect("persisted verification");
        assert_eq!(persisted.evidence_events.len(), 1);
        assert_eq!(persisted.evidence_events[0].event_type, "evidence.created");
        assert_eq!(persisted.verdict_event.event_type, "verification.verdict");
        assert_eq!(persisted.accepted_event.event_type, "task.accepted");
        assert_eq!(
            persisted.accepted_event.causation_id.as_deref(),
            Some(persisted.verdict_event.event_id.as_str())
        );
        state
            .verify_project_integrity("project-a")
            .expect("journal integrity");
    }
}
