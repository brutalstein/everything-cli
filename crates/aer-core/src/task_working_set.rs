//! Ephemeral long-horizon task working set.
//!
//! This is a derived projection over existing Engineering State / Handoff facts;
//! it is deliberately not a persistence or memory subsystem.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingEvidence {
    pub identity: String,
    pub content_hash: String,
    pub source_ref: String,
}

impl WorkingEvidence {
    #[must_use]
    pub fn new(
        identity: impl Into<String>,
        content_hash: impl Into<String>,
        source_ref: impl Into<String>,
    ) -> Self {
        Self {
            identity: identity.into(),
            content_hash: content_hash.into(),
            source_ref: source_ref.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TaskWorkingSet {
    pub task_id: String,
    pub edit_targets: BTreeSet<String>,
    pub relevant_symbols: BTreeSet<String>,
    pub verified_facts: BTreeSet<String>,
    pub architecture_constraints: BTreeSet<String>,
    pub latest_failures: BTreeSet<String>,
    pub relevant_tests: BTreeSet<String>,
    pub changed_files: BTreeSet<String>,
    pub unresolved_hypotheses: BTreeSet<String>,
    pub evidence: BTreeMap<String, WorkingEvidence>,
}

impl TaskWorkingSet {
    #[must_use]
    pub fn new(task_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            ..Self::default()
        }
    }

    pub fn insert_evidence(&mut self, evidence: WorkingEvidence) -> Option<WorkingEvidence> {
        self.evidence.insert(evidence.identity.clone(), evidence)
    }

    #[must_use]
    pub fn semantic_evidence_identity(&self) -> Vec<(String, String)> {
        self.evidence
            .values()
            .map(|evidence| (evidence.identity.clone(), evidence.content_hash.clone()))
            .collect()
    }

    #[must_use]
    pub fn delta_from(&self, previous: &Self) -> ContextDelta {
        ContextDelta::between(previous, self)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContextDelta {
    pub added: Vec<WorkingEvidence>,
    pub changed: Vec<EvidenceChange>,
    pub removed: Vec<WorkingEvidence>,
    pub invalidated: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceChange {
    pub identity: String,
    pub previous_hash: String,
    pub current: WorkingEvidence,
}

impl ContextDelta {
    #[must_use]
    pub fn between(previous: &TaskWorkingSet, current: &TaskWorkingSet) -> Self {
        let mut delta = Self::default();
        for (identity, evidence) in &current.evidence {
            match previous.evidence.get(identity) {
                None => delta.added.push(evidence.clone()),
                Some(old) if old.content_hash != evidence.content_hash => {
                    delta.changed.push(EvidenceChange {
                        identity: identity.clone(),
                        previous_hash: old.content_hash.clone(),
                        current: evidence.clone(),
                    });
                    delta.invalidated.push(identity.clone());
                }
                Some(_) => {}
            }
        }
        for (identity, evidence) in &previous.evidence {
            if !current.evidence.contains_key(identity) {
                delta.removed.push(evidence.clone());
                delta.invalidated.push(identity.clone());
            }
        }
        delta
            .added
            .sort_by(|left, right| left.identity.cmp(&right.identity));
        delta
            .changed
            .sort_by(|left, right| left.identity.cmp(&right.identity));
        delta
            .removed
            .sort_by(|left, right| left.identity.cmp(&right.identity));
        delta.invalidated.sort();
        delta.invalidated.dedup();
        delta
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.changed.is_empty()
            && self.removed.is_empty()
            && self.invalidated.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(id: &str, hash: &str) -> WorkingEvidence {
        WorkingEvidence::new(id, hash, format!("source:{id}"))
    }

    #[test]
    fn unchanged_evidence_retains_stable_identity_across_metadata_churn() {
        let mut before = TaskWorkingSet::new("task");
        before.insert_evidence(evidence("src:a", "sha256:a"));
        let mut after = before.clone();
        after.latest_failures.insert("new diagnostic".to_owned());
        after.changed_files.insert("unrelated.txt".to_owned());
        assert_eq!(
            before.semantic_evidence_identity(),
            after.semantic_evidence_identity()
        );
        assert!(after.delta_from(&before).is_empty());
    }

    #[test]
    fn changed_evidence_invalidates_immediately() {
        let mut before = TaskWorkingSet::new("task");
        before.insert_evidence(evidence("src:a", "sha256:a"));
        let mut after = TaskWorkingSet::new("task");
        after.insert_evidence(evidence("src:a", "sha256:b"));
        let delta = after.delta_from(&before);
        assert_eq!(delta.changed.len(), 1);
        assert_eq!(delta.invalidated, vec!["src:a"]);
        assert_eq!(delta.changed[0].previous_hash, "sha256:a");
        assert_eq!(delta.changed[0].current.content_hash, "sha256:b");
    }

    #[test]
    fn removed_and_added_evidence_are_explicit_and_deterministic() {
        let mut before = TaskWorkingSet::new("task");
        before.insert_evidence(evidence("b", "sha256:b"));
        let mut after = TaskWorkingSet::new("task");
        after.insert_evidence(evidence("a", "sha256:a"));
        let delta = ContextDelta::between(&before, &after);
        assert_eq!(delta.added[0].identity, "a");
        assert_eq!(delta.removed[0].identity, "b");
        assert_eq!(delta.invalidated, vec!["b"]);
    }
}
