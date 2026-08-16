from pathlib import Path

scheduler = Path("crates/aer-domain/src/scheduling.rs")
s = scheduler.read_text(encoding="utf-8")

anchor = '''    pub preemption: PreemptionSafety,
    pub priority_score: i64,
}'''
replacement = '''    pub preemption: PreemptionSafety,
    pub risk: TaskRisk,
    pub serial_only: bool,
    pub priority_score: i64,
}'''
if s.count(anchor) != 1:
    raise SystemExit(f"active fields anchor count {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)

anchor = '''            effect_class: task.effect_class,
            preemption: task.preemption,
            priority_score: task.priority.score(),
        }'''
replacement = '''            effect_class: task.effect_class,
            preemption: task.preemption,
            risk: task.risk,
            serial_only: task.serial_only,
            priority_score: task.priority.score(),
        }'''
if s.count(anchor) != 1:
    raise SystemExit(f"active constructor anchor count {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)

anchor = '''        if matches!(task.resource_estimate, ResourceEstimate::Unknown) {
            return Some(ScheduleBlockReason::UnknownResourceDemand);
        }
        if task.serial_only && (!self.active.is_empty() || !selected.is_empty()) {
'''
replacement = '''        if matches!(task.resource_estimate, ResourceEstimate::Unknown) {
            return Some(ScheduleBlockReason::UnknownResourceDemand);
        }
        if self.active.values().chain(selected.iter()).any(|active| {
            active.serial_only
                || (self.policy.serialize_high_risk
                    && matches!(active.risk, TaskRisk::High | TaskRisk::Critical))
        }) {
            return Some(ScheduleBlockReason::SerializationBarrier);
        }
        if task.serial_only && (!self.active.is_empty() || !selected.is_empty()) {
'''
if s.count(anchor) != 1:
    raise SystemExit(f"block reason anchor count {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)
scheduler.write_text(s, encoding="utf-8")

bench = Path("crates/aer-core/tests/resource_bench.rs")
s = bench.read_text(encoding="utf-8")
marker = '''#[test]
fn resource_bench_preemption_and_orphan_cleanup_are_safe_and_bounded() {
'''
addition = '''#[test]
fn resource_bench_high_risk_serialization_is_bidirectional() {
    let mut high = task("high", "run-a", "src/high", AdmissionClass::Generator);
    high.risk = TaskRisk::High;
    let low = task("low", "run-b", "src/low", AdmissionClass::Generator);

    let mut high_first = coordinator(2);
    high_first.register_task(&high, 1).expect("high register");
    high_first.register_task(&low, 1).expect("low register");
    high_first
        .admit_task(&high, "high-worker", 1)
        .expect("high-risk task may run alone");
    assert!(
        high_first.admit_task(&low, "low-worker", 2).is_err(),
        "an already-running high-risk task must serialize later admissions"
    );

    let mut low_first = coordinator(2);
    low_first.register_task(&low, 1).expect("low register");
    low_first.register_task(&high, 1).expect("high register");
    low_first
        .admit_task(&low, "low-worker", 1)
        .expect("low task starts");
    assert!(
        low_first.admit_task(&high, "high-worker", 2).is_err(),
        "high-risk admission must also refuse to join existing work"
    );
}

'''
if s.count(marker) != 1:
    raise SystemExit(f"ResourceBench preemption marker count {s.count(marker)}")
s = s.replace(marker, addition + marker, 1)
bench.write_text(s, encoding="utf-8")
