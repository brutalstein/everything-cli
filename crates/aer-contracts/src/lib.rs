//! Executable contract infrastructure for AER.
//!
//! This crate deliberately contains no model-provider or runtime-state dependency.
//! It turns the architecture's checked-in schemas and cross-object invariants into
//! deterministic validation primitives that later runtime layers can reuse.

pub mod benchmark;
pub mod proof_integrity;
pub mod schema;
pub mod semantic;
pub mod telemetry;

/// Runs the complete deterministic semantic validation stack for a bundle.
///
/// Runtime authority boundaries should use this facade rather than selecting
/// individual semantic passes. It composes reference/DAG validation with proof
/// integrity so adding a new deterministic pass cannot be silently skipped by
/// callers that follow the public contract entry point.
#[must_use]
pub fn validate_semantic_bundle(
    bundle: &semantic::SemanticBundle,
) -> Vec<semantic::SemanticIssue> {
    let mut issues = bundle.validate();
    issues.extend(proof_integrity::validate(bundle));
    issues
}
