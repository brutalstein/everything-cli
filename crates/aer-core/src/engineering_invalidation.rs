use crate::engineering_state::{
    EngineeringStateError, EngineeringStateStore, InvalidationReason, MemoryValidity,
};

impl EngineeringStateStore {
    pub fn invalidate_spec_change(
        &mut self,
        previous: &str,
        current: &str,
    ) -> Result<Vec<String>, EngineeringStateError> {
        if previous == current {
            return Ok(Vec::new());
        }
        let candidates = self
            .projection()?
            .records
            .into_iter()
            .filter(|record| record.validity == MemoryValidity::Current)
            .filter(|record| {
                record
                    .invalidation_scope
                    .spec_versions
                    .iter()
                    .any(|version| version == previous)
            })
            .map(|record| record.record_id)
            .collect::<Vec<_>>();
        for record_id in &candidates {
            self.invalidate(
                record_id,
                InvalidationReason::SpecChanged {
                    previous: previous.to_owned(),
                    current: current.to_owned(),
                },
            )?;
        }
        Ok(candidates)
    }

    pub fn invalidate_environment_change(
        &mut self,
        previous: &str,
        current: &str,
    ) -> Result<Vec<String>, EngineeringStateError> {
        if previous == current {
            return Ok(Vec::new());
        }
        let candidates = self
            .projection()?
            .records
            .into_iter()
            .filter(|record| record.validity == MemoryValidity::Current)
            .filter(|record| {
                record
                    .invalidation_scope
                    .environment_fingerprints
                    .iter()
                    .any(|fingerprint| fingerprint == previous)
            })
            .map(|record| record.record_id)
            .collect::<Vec<_>>();
        for record_id in &candidates {
            self.invalidate(
                record_id,
                InvalidationReason::EnvironmentChanged {
                    previous: previous.to_owned(),
                    current: current.to_owned(),
                },
            )?;
        }
        Ok(candidates)
    }

    pub fn invalidate_producer_change(
        &mut self,
        producer: &str,
    ) -> Result<Vec<String>, EngineeringStateError> {
        let candidates = self
            .projection()?
            .records
            .into_iter()
            .filter(|record| record.validity == MemoryValidity::Current)
            .filter(|record| {
                record
                    .invalidation_scope
                    .producer_identities
                    .iter()
                    .any(|identity| identity == producer)
            })
            .map(|record| record.record_id)
            .collect::<Vec<_>>();
        for record_id in &candidates {
            self.invalidate(
                record_id,
                InvalidationReason::ProducerChanged {
                    producer: producer.to_owned(),
                },
            )?;
        }
        Ok(candidates)
    }

    pub fn invalidate_dependency_change(
        &mut self,
        dependency: &str,
    ) -> Result<Vec<String>, EngineeringStateError> {
        let candidates = self
            .projection()?
            .records
            .into_iter()
            .filter(|record| record.validity == MemoryValidity::Current)
            .filter(|record| {
                record
                    .invalidation_scope
                    .dependency_identities
                    .iter()
                    .any(|identity| identity == dependency)
            })
            .map(|record| record.record_id)
            .collect::<Vec<_>>();
        for record_id in &candidates {
            self.invalidate(
                record_id,
                InvalidationReason::DependencyChanged {
                    dependency: dependency.to_owned(),
                },
            )?;
        }
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::atomic::{AtomicU64, Ordering}};

    use crate::engineering_state::{
        EngineeringMemoryRecord, InvalidationScope, MemoryKind, MemoryValidity,
    };

    use super::*;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn store(label: &str) -> (std::path::PathBuf, EngineeringStateStore) {
        let path = std::env::temp_dir().join(format!(
            "aer-engineering-invalidation-{}-{label}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create state root");
        let store = EngineeringStateStore::open(&path, format!("project-{label}"))
            .expect("open engineering state");
        (path, store)
    }

    fn scoped_record(id: &str) -> EngineeringMemoryRecord {
        EngineeringMemoryRecord {
            record_id: id.to_owned(),
            kind: MemoryKind::VerifiedFact,
            statement: "A verified conclusion bound to external context.".to_owned(),
            validity: MemoryValidity::Current,
            confidence_milli: 1000,
            repo_snapshot: Some("repo-v1".to_owned()),
            spec_version: Some("spec-v1".to_owned()),
            evidence_refs: vec![format!("evidence:{id}")],
            repository_entities: Vec::new(),
            invalidation_scope: InvalidationScope {
                repository_entity_ids: Vec::new(),
                spec_versions: vec!["spec-v1".to_owned()],
                environment_fingerprints: vec!["env-v1".to_owned()],
                producer_identities: vec!["parser:rust@1".to_owned()],
                dependency_identities: vec!["crate:serde@1".to_owned()],
            },
            hypothesis_state: None,
            failure_class: None,
            supersedes: None,
        }
    }

    #[test]
    fn unchanged_dimension_does_not_invalidate_memory() {
        let (root, mut store) = store("unchanged");
        store.record(&scoped_record("fact")).expect("record fact");
        assert!(store.invalidate_environment_change("env-v1", "env-v1").expect("invalidate").is_empty());
        assert_eq!(store.projection().expect("projection").records[0].validity, MemoryValidity::Current);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn spec_environment_dependency_and_producer_changes_invalidate_scoped_memory() {
        for (label, invalidate) in [
            ("spec", 0_u8),
            ("environment", 1_u8),
            ("producer", 2_u8),
            ("dependency", 3_u8),
        ] {
            let (root, mut store) = store(label);
            store.record(&scoped_record("fact")).expect("record fact");
            let invalidated = match invalidate {
                0 => store.invalidate_spec_change("spec-v1", "spec-v2"),
                1 => store.invalidate_environment_change("env-v1", "env-v2"),
                2 => store.invalidate_producer_change("parser:rust@1"),
                _ => store.invalidate_dependency_change("crate:serde@1"),
            }
            .expect("invalidate scoped fact");
            assert_eq!(invalidated, vec!["fact".to_owned()]);
            assert_eq!(
                store.projection().expect("projection").records[0].validity,
                MemoryValidity::Invalidated
            );
            fs::remove_dir_all(root).expect("cleanup");
        }
    }
}
