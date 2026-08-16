from pathlib import Path

p = Path("crates/aer-domain/src/scheduling.rs")
s = p.read_text(encoding="utf-8")

old = '''        let mut candidates = graph
            .tasks()
            .filter(|task| task.state == TaskState::Ready)
            .collect::<Vec<_>>();'''
new = '''        let mut candidates = graph
            .tasks()
            .filter(|task| {
                task.state == TaskState::Ready && !self.active.contains_key(&task.task_id)
            })
            .collect::<Vec<_>>();'''
if old not in s:
    raise SystemExit("candidate filter anchor missing")
s = s.replace(old, new, 1)

old = '''    SerialOnly,
    HighRiskSerialization,
    PredictedWriteOverlap { with_task: String },'''
new = '''    SerialOnly,
    HighRiskSerialization,
    WaveCapacity,
    SerializationBarrier,
    PredictedWriteOverlap { with_task: String },'''
if old not in s:
    raise SystemExit("reason enum anchor missing")
s = s.replace(old, new, 1)

old = '''        let mut service_shadow = self.run_service_units.clone();
        let mut selected_active = Vec::<ActiveTask>::new();

        while !candidates.is_empty() && wave.selected.len() < available {'''
new = '''        let mut service_shadow = self.run_service_units.clone();
        let mut selected_active = Vec::<ActiveTask>::new();
        let mut serialization_barrier = false;

        while !candidates.is_empty() && wave.selected.len() < available {'''
if old not in s:
    raise SystemExit("shadow anchor missing")
s = s.replace(old, new, 1)

old = '''            if task.serial_only
                || (self.policy.serialize_high_risk
                    && matches!(task.risk, TaskRisk::High | TaskRisk::Critical))
            {
                break;
            }
        }

        for task in candidates {
            push_deferred(
                &mut wave,
                self.policy.max_deferred_records,
                task,
                if task.serial_only {
                    ScheduleBlockReason::SerialOnly
                } else if self.policy.serialize_high_risk
                    && matches!(task.risk, TaskRisk::High | TaskRisk::Critical)
                {
                    ScheduleBlockReason::HighRiskSerialization
                } else {
                    ScheduleBlockReason::HighRiskSerialization
                },
            );
        }'''
new = '''            if task.serial_only
                || (self.policy.serialize_high_risk
                    && matches!(task.risk, TaskRisk::High | TaskRisk::Critical))
            {
                serialization_barrier = true;
                break;
            }
        }

        let remaining_reason = if serialization_barrier {
            ScheduleBlockReason::SerializationBarrier
        } else {
            ScheduleBlockReason::WaveCapacity
        };
        for task in candidates {
            push_deferred(
                &mut wave,
                self.policy.max_deferred_records,
                task,
                remaining_reason.clone(),
            );
        }'''
if old not in s:
    raise SystemExit("remaining candidate anchor missing")
s = s.replace(old, new, 1)

p.write_text(s, encoding="utf-8")
