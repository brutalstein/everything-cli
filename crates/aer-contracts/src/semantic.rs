//! Deterministic semantic validation across structurally valid core contracts.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// Cross-object input needed to validate the initial `REQ -> AC -> Task -> Evidence -> Proof` chain.
#[derive(Clone, Debug)]
pub struct SemanticBundle {
    pub engineering_ir: Value,
    pub tasks: Vec<Value>,
    pub evidence: Vec<Value>,
    pub proof_manifests: Vec<Value>,
}

/// Machine-stable semantic validation diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticIssue {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

impl SemanticBundle {
    /// Validates deterministic cross-object rules without model judgment.
    #[must_use]
    pub fn validate(&self) -> Vec<SemanticIssue> {
        let mut issues = Vec::new();

        let requirement_ids = collect_ids(
            &self.engineering_ir,
            "functional_requirements",
            "/engineering_ir/functional_requirements",
            &mut issues,
        );
        let acceptance_ids = collect_ids(
            &self.engineering_ir,
            "acceptance_criteria",
            "/engineering_ir/acceptance_criteria",
            &mut issues,
        );
        let invariant_ids = collect_ids(
            &self.engineering_ir,
            "invariants",
            "/engineering_ir/invariants",
            &mut issues,
        );

        validate_requirement_graph(&self.engineering_ir, &requirement_ids, &mut issues);
        validate_acceptance_references(&self.engineering_ir, &requirement_ids, &mut issues);

        let tasks_by_id = collect_objects_by_id(
            &self.tasks,
            "task_id",
            "/tasks",
            "semantic.duplicate_task_id",
            &mut issues,
        );
        validate_task_references(
            &self.tasks,
            &requirement_ids,
            &acceptance_ids,
            &invariant_ids,
            &tasks_by_id,
            &mut issues,
        );

        let evidence_by_id = collect_objects_by_id(
            &self.evidence,
            "evidence_id",
            "/evidence",
            "semantic.duplicate_evidence_id",
            &mut issues,
        );
        validate_evidence_references(&self.evidence, &requirement_ids, &mut issues);

        validate_proofs(
            &self.proof_manifests,
            &requirement_ids,
            &tasks_by_id,
            &evidence_by_id,
            &mut issues,
        );
        validate_accepted_tasks(&self.tasks, &self.proof_manifests, &mut issues);

        issues
    }
}

fn collect_ids(
    root: &Value,
    key: &str,
    path: &str,
    issues: &mut Vec<SemanticIssue>,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    if let Some(items) = root.get(key).and_then(Value::as_array) {
        for (index, item) in items.iter().enumerate() {
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                if !ids.insert(id.to_owned()) {
                    issues.push(issue(
                        "semantic.duplicate_id",
                        format!("{path}/{index}/id"),
                        format!("duplicate semantic id {id}"),
                    ));
                }
            }
        }
    }
    ids
}

fn collect_objects_by_id<'a>(
    objects: &'a [Value],
    id_key: &str,
    path: &str,
    duplicate_code: &'static str,
    issues: &mut Vec<SemanticIssue>,
) -> BTreeMap<String, &'a Value> {
    let mut by_id = BTreeMap::new();
    for (index, object) in objects.iter().enumerate() {
        if let Some(id) = object.get(id_key).and_then(Value::as_str) {
            if by_id.insert(id.to_owned(), object).is_some() {
                issues.push(issue(
                    duplicate_code,
                    format!("{path}/{index}/{id_key}"),
                    format!("duplicate {id_key} {id}"),
                ));
            }
        }
    }
    by_id
}

fn validate_requirement_graph(
    ir: &Value,
    requirement_ids: &BTreeSet<String>,
    issues: &mut Vec<SemanticIssue>,
) {
    let mut graph = BTreeMap::<String, Vec<String>>::new();
    if let Some(requirements) = ir.get("functional_requirements").and_then(Value::as_array) {
        for (index, requirement) in requirements.iter().enumerate() {
            let Some(id) = requirement.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut dependencies = Vec::new();
            if let Some(refs) = requirement.get("dependencies").and_then(Value::as_array) {
                for (ref_index, dependency) in refs.iter().enumerate() {
                    let Some(dependency) = dependency.as_str() else {
                        continue;
                    };
                    dependencies.push(dependency.to_owned());
                    if !requirement_ids.contains(dependency) {
                        issues.push(issue(
                            "semantic.dangling_requirement_dependency",
                            format!(
                                "/engineering_ir/functional_requirements/{index}/dependencies/{ref_index}"
                            ),
                            format!("requirement {id} depends on unknown requirement {dependency}"),
                        ));
                    }
                }
            }
            graph.insert(id.to_owned(), dependencies);
        }
    }

    if graph_has_cycle(&graph) {
        issues.push(issue(
            "semantic.requirement_cycle",
            "/engineering_ir/functional_requirements".to_owned(),
            "requirement dependency graph is cyclic".to_owned(),
        ));
    }
}

fn validate_acceptance_references(
    ir: &Value,
    requirement_ids: &BTreeSet<String>,
    issues: &mut Vec<SemanticIssue>,
) {
    if let Some(criteria) = ir.get("acceptance_criteria").and_then(Value::as_array) {
        for (index, criterion) in criteria.iter().enumerate() {
            if let Some(refs) = criterion.get("requirement_refs").and_then(Value::as_array) {
                for (ref_index, requirement_ref) in refs.iter().enumerate() {
                    let Some(requirement_ref) = requirement_ref.as_str() else {
                        continue;
                    };
                    if !requirement_ids.contains(requirement_ref) {
                        issues.push(issue(
                            "semantic.dangling_acceptance_requirement",
                            format!(
                                "/engineering_ir/acceptance_criteria/{index}/requirement_refs/{ref_index}"
                            ),
                            format!(
                                "acceptance criterion references unknown requirement {requirement_ref}"
                            ),
                        ));
                    }
                }
            }
        }
    }
}

fn validate_task_references(
    tasks: &[Value],
    requirement_ids: &BTreeSet<String>,
    acceptance_ids: &BTreeSet<String>,
    invariant_ids: &BTreeSet<String>,
    tasks_by_id: &BTreeMap<String, &Value>,
    issues: &mut Vec<SemanticIssue>,
) {
    let mut graph = BTreeMap::<String, Vec<String>>::new();
    for (index, task) in tasks.iter().enumerate() {
        validate_reference_array(
            task,
            "requirement_refs",
            requirement_ids,
            &format!("/tasks/{index}/requirement_refs"),
            "semantic.dangling_task_requirement",
            "requirement",
            issues,
        );
        validate_reference_array(
            task,
            "acceptance_refs",
            acceptance_ids,
            &format!("/tasks/{index}/acceptance_refs"),
            "semantic.dangling_task_acceptance",
            "acceptance criterion",
            issues,
        );
        validate_reference_array(
            task,
            "invariant_refs",
            invariant_ids,
            &format!("/tasks/{index}/invariant_refs"),
            "semantic.dangling_task_invariant",
            "invariant",
            issues,
        );

        let Some(task_id) = task.get("task_id").and_then(Value::as_str) else {
            continue;
        };
        let mut dependencies = Vec::new();
        if let Some(refs) = task.get("dependencies").and_then(Value::as_array) {
            for (ref_index, dependency) in refs.iter().enumerate() {
                let Some(dependency) = dependency.as_str() else {
                    continue;
                };
                dependencies.push(dependency.to_owned());
                if !tasks_by_id.contains_key(dependency) {
                    issues.push(issue(
                        "semantic.dangling_task_dependency",
                        format!("/tasks/{index}/dependencies/{ref_index}"),
                        format!("task {task_id} depends on unknown task {dependency}"),
                    ));
                }
            }
        }
        graph.insert(task_id.to_owned(), dependencies);
    }

    if graph_has_cycle(&graph) {
        issues.push(issue(
            "semantic.task_cycle",
            "/tasks".to_owned(),
            "task dependency graph is cyclic".to_owned(),
        ));
    }
}

fn validate_evidence_references(
    evidence: &[Value],
    requirement_ids: &BTreeSet<String>,
    issues: &mut Vec<SemanticIssue>,
) {
    for (index, record) in evidence.iter().enumerate() {
        validate_reference_array(
            record,
            "requirement_refs",
            requirement_ids,
            &format!("/evidence/{index}/requirement_refs"),
            "semantic.dangling_evidence_requirement",
            "requirement",
            issues,
        );
    }
}

fn validate_proofs(
    proofs: &[Value],
    requirement_ids: &BTreeSet<String>,
    tasks_by_id: &BTreeMap<String, &Value>,
    evidence_by_id: &BTreeMap<String, &Value>,
    issues: &mut Vec<SemanticIssue>,
) {
    for (proof_index, proof) in proofs.iter().enumerate() {
        let task_id = proof.get("task_id").and_then(Value::as_str);
        let task = task_id.and_then(|id| tasks_by_id.get(id).copied());
        if let Some(task_id) = task_id {
            if task.is_none() {
                issues.push(issue(
                    "semantic.dangling_proof_task",
                    format!("/proof_manifests/{proof_index}/task_id"),
                    format!("proof references unknown task {task_id}"),
                ));
            }
        }

        if let Some(task) = task {
            if proof.get("spec_version") != task.get("spec_version") {
                issues.push(issue(
                    "semantic.proof_spec_mismatch",
                    format!("/proof_manifests/{proof_index}/spec_version"),
                    "proof spec_version does not match its task".to_owned(),
                ));
            }
            if let Some(task_snapshot) = task.get("repo_snapshot").and_then(Value::as_str) {
                if proof.get("repo_snapshot").and_then(Value::as_str) != Some(task_snapshot) {
                    issues.push(issue(
                        "semantic.proof_repo_snapshot_mismatch",
                        format!("/proof_manifests/{proof_index}/repo_snapshot"),
                        "proof repo_snapshot does not match its task".to_owned(),
                    ));
                }
            }
        }

        let proof_snapshot = proof.get("repo_snapshot").and_then(Value::as_str);
        if let Some(requirements) = proof.get("requirements").and_then(Value::as_array) {
            let mut proof_requirement_ids = BTreeSet::new();
            for (requirement_index, requirement) in requirements.iter().enumerate() {
                if let Some(id) = requirement.get("id").and_then(Value::as_str) {
                    if !proof_requirement_ids.insert(id) {
                        issues.push(issue(
                            "semantic.duplicate_proof_requirement",
                            format!(
                                "/proof_manifests/{proof_index}/requirements/{requirement_index}/id"
                            ),
                            format!("proof contains duplicate requirement {id}"),
                        ));
                    }
                    if !requirement_ids.contains(id) {
                        issues.push(issue(
                            "semantic.dangling_proof_requirement",
                            format!(
                                "/proof_manifests/{proof_index}/requirements/{requirement_index}/id"
                            ),
                            format!("proof references unknown requirement {id}"),
                        ));
                    }
                }

                let verdict = requirement.get("verdict").and_then(Value::as_str);
                if let Some(refs) = requirement.get("evidence").and_then(Value::as_array) {
                    for (evidence_index, evidence_ref) in refs.iter().enumerate() {
                        let Some(evidence_ref) = evidence_ref.as_str() else {
                            continue;
                        };
                        let Some(evidence) = evidence_by_id.get(evidence_ref).copied() else {
                            issues.push(issue(
                                "semantic.dangling_proof_evidence",
                                format!(
                                    "/proof_manifests/{proof_index}/requirements/{requirement_index}/evidence/{evidence_index}"
                                ),
                                format!("proof references unknown evidence {evidence_ref}"),
                            ));
                            continue;
                        };
                        if evidence.get("repo_snapshot").and_then(Value::as_str) != proof_snapshot {
                            issues.push(issue(
                                "semantic.evidence_repo_snapshot_mismatch",
                                format!(
                                    "/proof_manifests/{proof_index}/requirements/{requirement_index}/evidence/{evidence_index}"
                                ),
                                format!(
                                    "evidence {evidence_ref} was produced for a different repo snapshot"
                                ),
                            ));
                        }
                        if verdict == Some("pass")
                            && evidence.get("result").and_then(Value::as_str) != Some("pass")
                        {
                            issues.push(issue(
                                "semantic.nonpassing_evidence_for_pass",
                                format!(
                                    "/proof_manifests/{proof_index}/requirements/{requirement_index}/evidence/{evidence_index}"
                                ),
                                format!(
                                    "passing requirement references non-passing evidence {evidence_ref}"
                                ),
                            ));
                        }
                    }
                }
            }
        }

        if proof.get("overall_verdict").and_then(Value::as_str) == Some("pass") {
            if let Some(requirements) = proof.get("requirements").and_then(Value::as_array) {
                for (requirement_index, requirement) in requirements.iter().enumerate() {
                    if requirement.get("verdict").and_then(Value::as_str) == Some("fail") {
                        issues.push(issue(
                            "semantic.passing_proof_contains_failed_requirement",
                            format!(
                                "/proof_manifests/{proof_index}/requirements/{requirement_index}/verdict"
                            ),
                            "overall passing proof contains a failed requirement".to_owned(),
                        ));
                    }
                }
            }
        }
    }
}

fn validate_accepted_tasks(
    tasks: &[Value],
    proofs: &[Value],
    issues: &mut Vec<SemanticIssue>,
) {
    for (index, task) in tasks.iter().enumerate() {
        if task.get("state").and_then(Value::as_str) != Some("accepted") {
            continue;
        }
        let Some(task_id) = task.get("task_id").and_then(Value::as_str) else {
            continue;
        };
        let has_passing_proof = proofs.iter().any(|proof| {
            proof.get("task_id").and_then(Value::as_str) == Some(task_id)
                && proof.get("overall_verdict").and_then(Value::as_str) == Some("pass")
        });
        if !has_passing_proof {
            issues.push(issue(
                "semantic.accepted_task_without_passing_proof",
                format!("/tasks/{index}/state"),
                format!("accepted task {task_id} has no passing proof manifest"),
            ));
        }
    }
}

fn validate_reference_array(
    object: &Value,
    key: &str,
    known_ids: &BTreeSet<String>,
    path: &str,
    code: &'static str,
    label: &str,
    issues: &mut Vec<SemanticIssue>,
) {
    if let Some(refs) = object.get(key).and_then(Value::as_array) {
        for (index, value) in refs.iter().enumerate() {
            let Some(reference) = value.as_str() else {
                continue;
            };
            if !known_ids.contains(reference) {
                issues.push(issue(
                    code,
                    format!("{path}/{index}"),
                    format!("unknown {label} reference {reference}"),
                ));
            }
        }
    }
}

fn graph_has_cycle(graph: &BTreeMap<String, Vec<String>>) -> bool {
    fn visit(
        node: &str,
        graph: &BTreeMap<String, Vec<String>>,
        temporary: &mut BTreeSet<String>,
        permanent: &mut BTreeSet<String>,
    ) -> bool {
        if permanent.contains(node) {
            return false;
        }
        if !temporary.insert(node.to_owned()) {
            return true;
        }
        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                if visit(neighbor, graph, temporary, permanent) {
                    return true;
                }
            }
        }
        temporary.remove(node);
        permanent.insert(node.to_owned());
        false
    }

    let mut temporary = BTreeSet::new();
    let mut permanent = BTreeSet::new();
    for node in graph.keys() {
        if visit(node, graph, &mut temporary, &mut permanent) {
            return true;
        }
    }
    false
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

    use super::SemanticBundle;

    fn valid_bundle() -> SemanticBundle {
        SemanticBundle {
            engineering_ir: json!({
                "schema_version": 1,
                "project": {"id": "p", "title": "P", "summary": "S"},
                "goals": [],
                "functional_requirements": [{
                    "id": "REQ-1",
                    "statement": "Requirement",
                    "priority": "must",
                    "dependencies": []
                }],
                "constraints": [],
                "invariants": [{"id": "INV-1", "statement": "Invariant"}],
                "acceptance_criteria": [{
                    "id": "AC-1",
                    "statement": "Criterion",
                    "requirement_refs": ["REQ-1"]
                }]
            }),
            tasks: vec![json!({
                "schema_version": 1,
                "task_id": "TASK-1",
                "kind": "implementation",
                "objective": "Implement",
                "requirement_refs": ["REQ-1"],
                "acceptance_refs": ["AC-1"],
                "invariant_refs": ["INV-1"],
                "dependencies": [],
                "risk": "low",
                "state": "accepted",
                "spec_version": 1,
                "repo_snapshot": "repo:1"
            })],
            evidence: vec![json!({
                "schema_version": 1,
                "evidence_id": "EVID-1",
                "type": "test",
                "requirement_refs": ["REQ-1"],
                "repo_snapshot": "repo:1",
                "result": "pass",
                "timestamp": "2026-08-15T00:00:00Z"
            })],
            proof_manifests: vec![json!({
                "schema_version": 1,
                "task_id": "TASK-1",
                "repo_snapshot": "repo:1",
                "spec_version": 1,
                "requirements": [{
                    "id": "REQ-1",
                    "implementation": [{"path": "src/lib.rs"}],
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
    fn valid_requirement_to_proof_chain_has_no_semantic_issues() {
        assert!(valid_bundle().validate().is_empty());
    }

    #[test]
    fn dangling_acceptance_reference_is_rejected() {
        let mut bundle = valid_bundle();
        bundle.engineering_ir["acceptance_criteria"][0]["requirement_refs"] =
            json!(["REQ-MISSING"]);
        let issues = bundle.validate();
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "semantic.dangling_acceptance_requirement")
        );
    }

    #[test]
    fn cyclic_task_graph_is_rejected() {
        let mut bundle = valid_bundle();
        bundle.tasks[0]["state"] = json!("ready");
        bundle.tasks[0]["dependencies"] = json!(["TASK-2"]);
        bundle.tasks.push(json!({
            "schema_version": 1,
            "task_id": "TASK-2",
            "kind": "implementation",
            "objective": "Second",
            "dependencies": ["TASK-1"],
            "risk": "low",
            "state": "ready",
            "spec_version": 1,
            "repo_snapshot": "repo:1"
        }));
        let issues = bundle.validate();
        assert!(issues.iter().any(|issue| issue.code == "semantic.task_cycle"));
    }

    #[test]
    fn passing_proof_cannot_use_failing_evidence() {
        let mut bundle = valid_bundle();
        bundle.evidence[0]["result"] = json!("fail");
        let issues = bundle.validate();
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == "semantic.nonpassing_evidence_for_pass")
        );
    }
}
