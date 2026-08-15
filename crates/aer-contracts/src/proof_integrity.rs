//! Deterministic integrity rules for the Task -> Evidence -> Proof boundary.
//!
//! Structural schemas establish shape. These checks establish that a passing
//! proof is actually about the task and requirements it claims to verify.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::semantic::{SemanticBundle, SemanticIssue};

/// Validates proof scope, evidence relevance, and verifier immutability.
#[must_use]
pub fn validate(bundle: &SemanticBundle) -> Vec<SemanticIssue> {
    let tasks_by_id = index_by(&bundle.tasks, "task_id");
    let evidence_by_id = index_by(&bundle.evidence, "evidence_id");
    let mut issues = Vec::new();

    for (proof_index, proof) in bundle.proof_manifests.iter().enumerate() {
        let task = proof
            .get("task_id")
            .and_then(Value::as_str)
            .and_then(|task_id| tasks_by_id.get(task_id).copied());
        let mut proof_requirement_ids = BTreeSet::new();

        if let Some(requirements) = proof.get("requirements").and_then(Value::as_array) {
            for (requirement_index, requirement) in requirements.iter().enumerate() {
                let requirement_id = requirement.get("id").and_then(Value::as_str);
                if let Some(requirement_id) = requirement_id {
                    proof_requirement_ids.insert(requirement_id.to_owned());

                    if let Some(task) = task
                        && !string_array_contains(task, "requirement_refs", requirement_id)
                    {
                        issues.push(issue(
                            "semantic.proof_requirement_not_in_task",
                            format!(
                                "/proof_manifests/{proof_index}/requirements/{requirement_index}/id"
                            ),
                            format!(
                                "proof requirement {requirement_id} is outside the referenced task scope"
                            ),
                        ));
                    }
                }

                let verdict = requirement.get("verdict").and_then(Value::as_str);
                let evidence_refs = requirement.get("evidence").and_then(Value::as_array);
                if verdict == Some("pass") && evidence_refs.is_none_or(|refs| refs.is_empty()) {
                    issues.push(issue(
                        "semantic.pass_without_evidence",
                        format!(
                            "/proof_manifests/{proof_index}/requirements/{requirement_index}/evidence"
                        ),
                        "a passing requirement must reference evidence".to_owned(),
                    ));
                }

                if let Some(evidence_refs) = evidence_refs {
                    for (evidence_index, evidence_ref) in evidence_refs.iter().enumerate() {
                        let Some(evidence_ref) = evidence_ref.as_str() else {
                            continue;
                        };
                        let Some(evidence) = evidence_by_id.get(evidence_ref).copied() else {
                            continue;
                        };

                        if let Some(requirement_id) = requirement_id
                            && !string_array_contains(evidence, "requirement_refs", requirement_id)
                        {
                            issues.push(issue(
                                "semantic.evidence_requirement_mismatch",
                                format!(
                                    "/proof_manifests/{proof_index}/requirements/{requirement_index}/evidence/{evidence_index}"
                                ),
                                format!(
                                    "evidence {evidence_ref} does not attest requirement {requirement_id}"
                                ),
                            ));
                        }
                    }
                }
            }
        }

        if let Some(task) = task {
            for requirement_id in string_array(task, "requirement_refs") {
                if !proof_requirement_ids.contains(requirement_id) {
                    issues.push(issue(
                        "semantic.missing_proof_requirement",
                        format!("/proof_manifests/{proof_index}/requirements"),
                        format!(
                            "proof omits task requirement {requirement_id} from its verification scope"
                        ),
                    ));
                }
            }
        }

        if proof.get("overall_verdict").and_then(Value::as_str) == Some("pass")
            && proof
                .get("integrity")
                .and_then(|integrity| integrity.get("generator_could_modify_verifier"))
                .and_then(Value::as_bool)
                == Some(true)
        {
            issues.push(issue(
                "semantic.mutable_verifier_for_pass",
                format!(
                    "/proof_manifests/{proof_index}/integrity/generator_could_modify_verifier"
                ),
                "a passing proof cannot claim an independently trusted verifier while the generator could modify it"
                    .to_owned(),
            ));
        }
    }

    issues
}

fn index_by<'a>(objects: &'a [Value], id_key: &str) -> BTreeMap<&'a str, &'a Value> {
    objects
        .iter()
        .filter_map(|object| {
            object
                .get(id_key)
                .and_then(Value::as_str)
                .map(|id| (id, object))
        })
        .collect()
}

fn string_array<'a>(object: &'a Value, key: &str) -> Vec<&'a str> {
    object
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn string_array_contains(object: &Value, key: &str, expected: &str) -> bool {
    object
        .get(key)
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}

fn issue(code: &'static str, path: String, message: String) -> SemanticIssue {
    SemanticIssue {
        code,
        path,
        message,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::validate;
    use crate::semantic::SemanticBundle;

    fn valid_bundle() -> SemanticBundle {
        SemanticBundle {
            engineering_ir: json!({}),
            tasks: vec![json!({
                "task_id": "TASK-1",
                "requirement_refs": ["REQ-1"],
                "state": "accepted",
                "spec_version": 1,
                "repo_snapshot": "repo:1"
            })],
            evidence: vec![json!({
                "evidence_id": "EVID-1",
                "requirement_refs": ["REQ-1"],
                "repo_snapshot": "repo:1",
                "result": "pass"
            })],
            proof_manifests: vec![json!({
                "task_id": "TASK-1",
                "repo_snapshot": "repo:1",
                "spec_version": 1,
                "requirements": [{
                    "id": "REQ-1",
                    "evidence": ["EVID-1"],
                    "verdict": "pass"
                }],
                "integrity": {
                    "verifier_snapshot": "verifier:1",
                    "generator_could_modify_verifier": false
                },
                "overall_verdict": "pass"
            })],
        }
    }

    #[test]
    fn aligned_proof_scope_and_evidence_are_valid() {
        assert!(validate(&valid_bundle()).is_empty());
    }

    #[test]
    fn passing_requirement_requires_evidence() {
        let mut bundle = valid_bundle();
        bundle.proof_manifests[0]["requirements"][0]["evidence"] = json!([]);
        assert!(
            validate(&bundle)
                .iter()
                .any(|issue| issue.code == "semantic.pass_without_evidence")
        );
    }

    #[test]
    fn evidence_must_attest_the_requirement_it_proves() {
        let mut bundle = valid_bundle();
        bundle.evidence[0]["requirement_refs"] = json!(["REQ-OTHER"]);
        assert!(
            validate(&bundle)
                .iter()
                .any(|issue| issue.code == "semantic.evidence_requirement_mismatch")
        );
    }

    #[test]
    fn proof_must_cover_every_requirement_in_task_scope() {
        let mut bundle = valid_bundle();
        bundle.tasks[0]["requirement_refs"] = json!(["REQ-1", "REQ-2"]);
        assert!(
            validate(&bundle)
                .iter()
                .any(|issue| issue.code == "semantic.missing_proof_requirement")
        );
    }

    #[test]
    fn proof_cannot_expand_beyond_task_requirement_scope() {
        let mut bundle = valid_bundle();
        bundle.proof_manifests[0]["requirements"][0]["id"] = json!("REQ-2");
        bundle.evidence[0]["requirement_refs"] = json!(["REQ-2"]);
        assert!(
            validate(&bundle)
                .iter()
                .any(|issue| issue.code == "semantic.proof_requirement_not_in_task")
        );
    }

    #[test]
    fn mutable_verifier_cannot_support_passing_proof() {
        let mut bundle = valid_bundle();
        bundle.proof_manifests[0]["integrity"]["generator_could_modify_verifier"] = json!(true);
        assert!(
            validate(&bundle)
                .iter()
                .any(|issue| issue.code == "semantic.mutable_verifier_for_pass")
        );
    }
}
