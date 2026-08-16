# One-shot source repair; removed before Step 13 merge.
from pathlib import Path

core = Path("crates/aer-core/src/parallel.rs")
s = core.read_text(encoding="utf-8")
old = '''        if !self.records.contains_key(&resource.resource_id)
            && self.records.len() >= self.max_records
        {
            return Err(OrphanError::RegistryFull);
        }
        if self
            .records
            .insert(resource.resource_id.clone(), resource)
            .is_some()
        {
            return Err(OrphanError::DuplicateResource);
        }
        Ok(())'''
new = '''        if self.records.contains_key(&resource.resource_id) {
            return Err(OrphanError::DuplicateResource);
        }
        if self.records.len() >= self.max_records {
            return Err(OrphanError::RegistryFull);
        }
        self.records.insert(resource.resource_id.clone(), resource);
        Ok(())'''
if s.count(old) != 1:
    raise SystemExit(f"expected one orphan register block, found {s.count(old)}")
s = s.replace(old, new, 1)
core.write_text(s, encoding="utf-8")

bench = Path("crates/aer-core/tests/resource_bench.rs")
s = bench.read_text(encoding="utf-8")
anchor = '''        })
        .expect("register live");
    registry
        .register(OwnedRuntimeResource {
            resource_id: "orphan-a".to_owned(),'''
replacement = '''        })
        .expect("register live");
    assert!(registry
        .register(OwnedRuntimeResource {
            resource_id: "live-worktree".to_owned(),
            task_id: "dead-shadow".to_owned(),
            kind: OwnedResourceKind::Container,
            cleanup_deadline_ms: 0,
        })
        .is_err(), "duplicate registration must fail before mutation");
    registry
        .register(OwnedRuntimeResource {
            resource_id: "orphan-a".to_owned(),'''
if s.count(anchor) != 1:
    raise SystemExit(f"expected one ResourceBench live anchor, found {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)
anchor = '''    let third = registry.reconcile(&live, 3, |_| true);
    assert_eq!(third.cleaned.len(), 1);
    assert_eq!(registry.len(), 1, "live owner resource must never be swept");
}'''
replacement = '''    let third = registry.reconcile(&live, 3, |_| true);
    assert_eq!(third.cleaned.len(), 1);
    assert_eq!(registry.len(), 1, "live owner resource must never be swept");
    let remaining = registry.release("live-worktree").expect("live resource remains");
    assert_eq!(remaining.task_id, "external", "duplicate failure must not overwrite authority");
}'''
if s.count(anchor) != 1:
    raise SystemExit(f"expected one ResourceBench final orphan anchor, found {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)
bench.write_text(s, encoding="utf-8")
