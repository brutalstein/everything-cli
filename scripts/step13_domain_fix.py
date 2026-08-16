# One-shot source repair; removed before Step 13 merge.
from pathlib import Path

workspace = Path("crates/aer-workspace/src/parallel.rs")
s = workspace.read_text(encoding="utf-8")
old = '''        let merge = run_git(
            &self.owned.path,
            [
                OsString::from("merge"),
                OsString::from("--no-ff"),
                OsString::from("--no-edit"),
                OsString::from("--no-verify"),
                OsString::from(&changes.branch_name),
            ],
            SideEffectClass::WorkspaceWrite,
            None,
            INSPECTION_OUTPUT_LIMIT,
        );'''
new = '''        let merge = run_git_with_env(
            &self.owned.path,
            [
                OsString::from("merge"),
                OsString::from("--no-ff"),
                OsString::from("--no-edit"),
                OsString::from("--no-verify"),
                OsString::from(&changes.branch_name),
            ],
            SideEffectClass::WorkspaceWrite,
            &[
                ("GIT_AUTHOR_NAME", INTERNAL_AUTHOR_NAME),
                ("GIT_AUTHOR_EMAIL", INTERNAL_AUTHOR_EMAIL),
                ("GIT_COMMITTER_NAME", INTERNAL_AUTHOR_NAME),
                ("GIT_COMMITTER_EMAIL", INTERNAL_AUTHOR_EMAIL),
                ("GIT_AUTHOR_DATE", INTERNAL_SNAPSHOT_DATE),
                ("GIT_COMMITTER_DATE", INTERNAL_SNAPSHOT_DATE),
            ],
        );'''
if s.count(old) != 1:
    raise SystemExit(f"expected one integration merge block, found {s.count(old)}")
s = s.replace(old, new, 1)
workspace.write_text(s, encoding="utf-8")

bench = Path("crates/aer-core/tests/resource_bench.rs")
s = bench.read_text(encoding="utf-8")
old = '''    process::Command,
    time::SystemTime,
};'''
new = '''    process::Command,
    thread,
    time::{Duration, Instant, SystemTime},
};'''
if s.count(old) != 1:
    raise SystemExit(f"expected one ResourceBench import block, found {s.count(old)}")
s = s.replace(old, new, 1)
anchor = '''#[test]
fn resource_bench_measures_positive_parallel_utility_instead_of_assuming_it() {
    let policy = ParallelUtilityPolicy::new(100, 50).expect("utility policy");
    assert!(ParallelUtilityMeasurement {
        serial_wall_ms: 1_000,
        parallel_wall_ms: 600,
        coordination_ms: 100,
        serial_verified_successes: 2,
        parallel_verified_successes: 2,
        serial_cost_microusd: 100,
        parallel_cost_microusd: 130,
    }
    .positive(policy)
    .expect("positive utility"));
    assert!(!ParallelUtilityMeasurement {
        serial_wall_ms: 1_000,
        parallel_wall_ms: 700,
        coordination_ms: 250,
        serial_verified_successes: 2,
        parallel_verified_successes: 2,
        serial_cost_microusd: 100,
        parallel_cost_microusd: 200,
    }
    .positive(policy)
    .expect("negative utility"));
}
'''
addition = anchor + '''
#[test]
fn resource_bench_real_parallel_work_shows_measured_positive_utility() {
    const WORK: Duration = Duration::from_millis(200);

    let serial_started = Instant::now();
    thread::sleep(WORK);
    thread::sleep(WORK);
    let serial_wall_ms = serial_started.elapsed().as_millis() as u64;

    let parallel_started = Instant::now();
    thread::scope(|scope| {
        scope.spawn(|| thread::sleep(WORK));
        scope.spawn(|| thread::sleep(WORK));
    });
    let parallel_wall_ms = parallel_started.elapsed().as_millis() as u64;

    assert!(
        parallel_wall_ms < serial_wall_ms,
        "independent parallel work must beat its measured serial control: parallel={parallel_wall_ms}ms serial={serial_wall_ms}ms"
    );
    let policy = ParallelUtilityPolicy::new(200, 0).expect("measured utility policy");
    assert!(ParallelUtilityMeasurement {
        serial_wall_ms,
        parallel_wall_ms,
        coordination_ms: 0,
        serial_verified_successes: 2,
        parallel_verified_successes: 2,
        serial_cost_microusd: 100,
        parallel_cost_microusd: 100,
    }
    .positive(policy)
    .expect("measured parallel utility"));
}
'''
if s.count(anchor) != 1:
    raise SystemExit(f"expected one synthetic utility test anchor, found {s.count(anchor)}")
s = s.replace(anchor, addition, 1)
bench.write_text(s, encoding="utf-8")
