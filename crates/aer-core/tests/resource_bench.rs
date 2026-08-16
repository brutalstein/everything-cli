use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime},
};

use aer_core::parallel::{
    IntegrationBarrier, IntegrationPlan, IntegrationVerification, LocalBranchEvidence,
    OrphanRegistry, OwnedResourceKind, OwnedRuntimeResource, ParallelRuntimeCoordinator,
    ParallelUtilityMeasurement, ParallelUtilityPolicy,
};
use aer_domain::{
    leases::{EffectClass, LeasePolicy},
    resource_governor::{AdmissionClass, ResourceEstimate, ResourceLimits, ResourceVector},
    scheduling::{
        PreemptionSafety, PrioritySignals, SchedulableTask, SchedulerPolicy, SemanticWriteSet,
        TaskGraph, TaskRisk, WriteScope,
    },
    state_machines::TaskState,
};
use aer_workspace::{
    SnapshotPolicy, WorkspaceSnapshot,
    parallel::{TaskChangeSet, list_managed_worktrees},
};

fn demand() -> ResourceEstimate {
    ResourceEstimate::Known(ResourceVector {
        worker_slots: 1,
        cpu_millis: 100,
        memory_bytes: 1024,
        disk_bytes: 1024,
        pids: 2,
        open_files: 8,
        wall_time_ms: 10_000,
        network_bytes: 1024,
        local_ports: 0,
        provider_requests: 1,
        provider_input_tokens: 100,
        provider_output_tokens: 100,
        provider_concurrency: 1,
        monetary_microusd: 10,
        gpu_memory_bytes: 0,
    })
}

fn task(id: &str, run_id: &str, path: &str, class: AdmissionClass) -> SchedulableTask {
    SchedulableTask {
        task_id: id.to_owned(),
        run_id: run_id.to_owned(),
        state: TaskState::Ready,
        dependencies: BTreeSet::new(),
        admission_class: class,
        resource_estimate: demand(),
        effect_class: EffectClass::WorkspaceLocal,
        expected_write_scope: WriteScope::new([path]).expect("safe write scope"),
        semantic_write_set: SemanticWriteSet::default(),
        risk: TaskRisk::Low,
        contracts_stable: true,
        has_local_verifier: true,
        serial_only: false,
        preemption: PreemptionSafety::Checkpointable,
        priority: PrioritySignals::default(),
    }
}

fn coordinator(max_parallel: usize) -> ParallelRuntimeCoordinator {
    let limits = ResourceLimits::new(
        ResourceVector {
            worker_slots: 4,
            cpu_millis: 10_000,
            memory_bytes: 1024 * 1024,
            disk_bytes: 1024 * 1024,
            pids: 32,
            open_files: 256,
            wall_time_ms: 100_000,
            network_bytes: 1024 * 1024,
            local_ports: 8,
            provider_requests: 32,
            provider_input_tokens: 100_000,
            provider_output_tokens: 100_000,
            provider_concurrency: 4,
            monetary_microusd: 10_000,
            gpu_memory_bytes: 0,
        },
        1,
    )
    .expect("limits");
    ParallelRuntimeCoordinator::new(
        SchedulerPolicy::new(max_parallel, 128).expect("scheduler policy"),
        limits,
        LeasePolicy::new(1_000, 5_000).expect("lease policy"),
    )
}

#[test]
fn resource_bench_protects_verifier_capacity_and_rolls_back_denied_admission() {
    let generators = [
        task("g1", "run-a", "src/g1", AdmissionClass::Generator),
        task("g2", "run-a", "src/g2", AdmissionClass::Generator),
        task("g3", "run-a", "src/g3", AdmissionClass::Generator),
        task("g4", "run-a", "src/g4", AdmissionClass::Generator),
    ];
    let verifier = task(
        "verifier",
        "run-v",
        "verify/output",
        AdmissionClass::Verifier,
    );
    let mut runtime = coordinator(4);
    for candidate in generators.iter().chain(std::iter::once(&verifier)) {
        runtime.register_task(candidate, 1).expect("register task");
    }

    for (index, candidate) in generators.iter().take(3).enumerate() {
        runtime
            .admit_task(candidate, format!("worker-{index}"), 10)
            .expect("generator admission");
    }
    assert_eq!(runtime.active_count(), 3);
    assert_eq!(runtime.resource_usage().worker_slots, 3);

    assert!(runtime.admit_task(&generators[3], "worker-4", 10).is_err());
    assert_eq!(runtime.active_count(), 3, "denied admission must roll back");
    assert_eq!(runtime.resource_usage().worker_slots, 3);

    runtime
        .admit_task(&verifier, "verifier-worker", 10)
        .expect("verifier reserve must remain usable");
    assert_eq!(runtime.active_count(), 4);
    assert_eq!(runtime.resource_usage().worker_slots, 4);

    for task_id in ["g1", "g2", "g3", "verifier"] {
        runtime.begin_verification(task_id).expect("begin verify");
        assert_eq!(
            runtime
                .finalize_verification(task_id, true)
                .expect("finalize verify"),
            TaskState::Accepted
        );
    }
    assert_eq!(runtime.active_count(), 0);
    assert_eq!(runtime.resource_usage(), ResourceVector::default());
}

#[test]
fn resource_bench_scheduler_is_bounded_fair_and_detects_runtime_overlap() {
    let mut tasks = Vec::new();
    for index in 0..40 {
        let run = if index % 2 == 0 { "run-a" } else { "run-b" };
        tasks.push(task(
            &format!("t{index:02}"),
            run,
            &format!("src/task-{index:02}"),
            AdmissionClass::Generator,
        ));
    }
    let graph = TaskGraph::new(tasks).expect("task graph");
    let runtime = coordinator(3);
    let wave = runtime.plan_wave(&graph).expect("bounded wave");
    assert_eq!(wave.selected.len(), 3);
    let selected_runs = wave
        .selected
        .iter()
        .map(|decision| decision.run_id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(selected_runs.len(), 2, "fairness must expose both runs");

    let mut left = task("left", "run-a", "src/left", AdmissionClass::Generator);
    let mut right = task("right", "run-b", "src/right", AdmissionClass::Generator);
    left.priority.unblock_value = 10;
    right.priority.unblock_value = 9;
    let mut overlap_runtime = coordinator(2);
    overlap_runtime
        .register_task(&left, 1)
        .expect("left register");
    overlap_runtime
        .register_task(&right, 1)
        .expect("right register");
    overlap_runtime
        .admit_task(&left, "worker-left", 1)
        .expect("left admit");
    overlap_runtime
        .admit_task(&right, "worker-right", 1)
        .expect("right admit");
    assert!(
        overlap_runtime
            .observe_actual_write_scope(
                "left",
                WriteScope::new(["src/shared"]).expect("actual left")
            )
            .expect("left observation")
            .is_empty()
    );
    let conflicts = overlap_runtime
        .observe_actual_write_scope(
            "right",
            WriteScope::new(["src/shared/config.rs"]).expect("actual right"),
        )
        .expect("right observation");
    assert_eq!(conflicts.len(), 1);
    assert!(!conflicts[0].predicted_overlap);
}

#[test]
fn resource_bench_preemption_and_orphan_cleanup_are_safe_and_bounded() {
    let mut low = task("low", "run-a", "src/low", AdmissionClass::Generator);
    low.priority.unblock_value = 1;
    let mut external = task(
        "external",
        "run-a",
        "src/external",
        AdmissionClass::Generator,
    );
    external.effect_class = EffectClass::ExternalMutating;
    external.priority.unblock_value = 0;
    let mut incoming = task(
        "incoming",
        "run-b",
        "src/incoming",
        AdmissionClass::Generator,
    );
    incoming.priority.user_waiting_value = 100;

    let mut runtime = coordinator(3);
    runtime.register_task(&low, 1).expect("low register");
    runtime
        .register_task(&external, 1)
        .expect("external register");
    runtime
        .admit_task(&low, "low-worker", 10)
        .expect("low admit");
    runtime
        .admit_task(&external, "external-worker", 10)
        .expect("external admit");
    let preemption = runtime
        .request_preemption_for(&incoming, 20, 100)
        .expect("preemption request")
        .expect("safe preemption candidate");
    assert_eq!(preemption.preempted_task_id, "low");
    runtime
        .complete_cancellation("low")
        .expect("preemption cleanup");
    assert_eq!(runtime.active_count(), 1);

    let mut registry = OrphanRegistry::new(4, 1).expect("orphan policy");
    registry
        .register(OwnedRuntimeResource {
            resource_id: "live-worktree".to_owned(),
            task_id: "external".to_owned(),
            kind: OwnedResourceKind::Worktree,
            cleanup_deadline_ms: 0,
        })
        .expect("register live");
    assert!(
        registry
            .register(OwnedRuntimeResource {
                resource_id: "live-worktree".to_owned(),
                task_id: "dead-shadow".to_owned(),
                kind: OwnedResourceKind::Container,
                cleanup_deadline_ms: 0,
            })
            .is_err(),
        "duplicate registration must fail before mutation"
    );
    registry
        .register(OwnedRuntimeResource {
            resource_id: "orphan-a".to_owned(),
            task_id: "dead-a".to_owned(),
            kind: OwnedResourceKind::ProcessTree,
            cleanup_deadline_ms: 0,
        })
        .expect("register orphan a");
    registry
        .register(OwnedRuntimeResource {
            resource_id: "orphan-b".to_owned(),
            task_id: "dead-b".to_owned(),
            kind: OwnedResourceKind::EphemeralService,
            cleanup_deadline_ms: 0,
        })
        .expect("register orphan b");
    let live = BTreeSet::from(["external".to_owned()]);
    let first = registry.reconcile(&live, 1, |_| false);
    assert_eq!(first.failed.len(), 1);
    assert_eq!(first.deferred.len(), 1);
    assert_eq!(registry.len(), 3, "failed cleanup must remain durable");
    let second = registry.reconcile(&live, 2, |_| true);
    assert_eq!(second.cleaned.len(), 1);
    assert_eq!(second.deferred.len(), 1);
    let third = registry.reconcile(&live, 3, |_| true);
    assert_eq!(third.cleaned.len(), 1);
    assert_eq!(registry.len(), 1, "live owner resource must never be swept");
    let remaining = registry
        .release("live-worktree")
        .expect("live resource remains");
    assert_eq!(
        remaining.task_id, "external",
        "duplicate failure must not overwrite authority"
    );
}

fn change_set(task_id: &str, path: &str) -> LocalBranchEvidence {
    LocalBranchEvidence {
        task_id: task_id.to_owned(),
        changes: TaskChangeSet {
            branch_name: format!("aer/task/{task_id}"),
            base_commit: "base".to_owned(),
            head_commit: format!("head-{task_id}"),
            changed_paths: vec![PathBuf::from(path)],
            dirty: false,
        },
        semantic_write_set: SemanticWriteSet::default(),
        local_verification_passed: true,
        evidence_ids: BTreeSet::from([format!("evidence-{task_id}")]),
    }
}

#[test]
fn resource_bench_integration_rejects_semantic_conflict_and_requires_global_proof() {
    let mut left = change_set("left", "src/client.rs");
    let mut right = change_set("right", "src/server.rs");
    left.semantic_write_set =
        SemanticWriteSet::new(["public-api:auth"]).expect("left semantic set");
    right.semantic_write_set =
        SemanticWriteSet::new(["public-api:auth"]).expect("right semantic set");
    assert!(IntegrationPlan::build(vec![left, right], 4).is_err());

    let clean_left = change_set("left", "src/client.rs");
    let clean_right = change_set("right", "src/server.rs");
    let plan = IntegrationPlan::build(vec![clean_left, clean_right], 4).expect("clean plan");
    let mut barrier = IntegrationBarrier::new(plan);
    assert!(
        barrier
            .finalize(IntegrationVerification {
                passed: true,
                integration_head: "not-merged".to_owned(),
                proof_manifest_ids: BTreeSet::from(["proof".to_owned()]),
                environment_fingerprint: "env".to_owned(),
                repo_snapshot: "repo".to_owned(),
            })
            .is_err()
    );
    barrier
        .record_merge("left", "merge-left")
        .expect("merge left");
    barrier
        .record_merge("right", "merge-right")
        .expect("merge right");
    assert!(
        barrier
            .finalize(IntegrationVerification {
                passed: false,
                integration_head: "merge-right".to_owned(),
                proof_manifest_ids: BTreeSet::from(["proof".to_owned()]),
                environment_fingerprint: "env".to_owned(),
                repo_snapshot: "repo".to_owned(),
            })
            .is_err()
    );
    let accepted = barrier
        .finalize(IntegrationVerification {
            passed: true,
            integration_head: "merge-right".to_owned(),
            proof_manifest_ids: BTreeSet::from(["proof".to_owned()]),
            environment_fingerprint: "env".to_owned(),
            repo_snapshot: "repo".to_owned(),
        })
        .expect("integrated acceptance");
    assert_eq!(accepted.task_ids, vec!["left", "right"]);
}

#[test]
fn resource_bench_measures_positive_parallel_utility_instead_of_assuming_it() {
    let policy = ParallelUtilityPolicy::new(100, 50).expect("utility policy");
    assert!(
        ParallelUtilityMeasurement {
            serial_wall_ms: 1_000,
            parallel_wall_ms: 600,
            coordination_ms: 100,
            serial_verified_successes: 2,
            parallel_verified_successes: 2,
            serial_cost_microusd: 100,
            parallel_cost_microusd: 130,
        }
        .positive(policy)
        .expect("positive utility")
    );
    assert!(
        !ParallelUtilityMeasurement {
            serial_wall_ms: 1_000,
            parallel_wall_ms: 700,
            coordination_ms: 250,
            serial_verified_successes: 2,
            parallel_verified_successes: 2,
            serial_cost_microusd: 100,
            parallel_cost_microusd: 200,
        }
        .positive(policy)
        .expect("negative utility")
    );
}

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
    assert!(
        ParallelUtilityMeasurement {
            serial_wall_ms,
            parallel_wall_ms,
            coordination_ms: 0,
            serial_verified_successes: 2,
            parallel_verified_successes: 2,
            serial_cost_microusd: 100,
            parallel_cost_microusd: 100,
        }
        .positive(policy)
        .expect("measured parallel utility")
    );
}

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "everything-resource-bench-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp dir");
    path
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn initialized_repo() -> PathBuf {
    let repo = temp_dir("repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "Everything ResourceBench"]);
    git(
        &repo,
        &["config", "user.email", "resource-bench@everything.invalid"],
    );
    git(&repo, &["config", "core.autocrlf", "false"]);
    fs::create_dir_all(repo.join("src")).expect("src");
    fs::write(repo.join("src/base.txt"), "base\n").expect("base");
    fs::write(repo.join("src/left.txt"), "left-base\n").expect("left base");
    fs::write(repo.join("src/right.txt"), "right-base\n").expect("right base");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "initial"]);
    repo
}

#[test]
fn resource_bench_real_worktrees_preserve_user_state_and_integrate_verified_branches() {
    let repo = initialized_repo();
    fs::write(repo.join("src/base.txt"), "user-dirty\n").expect("dirty tracked");
    fs::write(repo.join("local-only.txt"), "do-not-touch\n").expect("dirty untracked");
    let user_branch = git(&repo, &["branch", "--show-current"]);

    let snapshot = WorkspaceSnapshot::capture(&repo, &SnapshotPolicy::default())
        .expect("capture exact dirty snapshot");
    let owned_root = temp_dir("owned");
    let integration_path = owned_root.join("integration");
    let mut integration = snapshot
        .materialize_integration_worktree(&integration_path, "aer/integration/resource-bench")
        .expect("integration worktree");
    let baseline = integration.snapshot_commit().to_owned();
    assert_ne!(baseline, snapshot.identity.head_commit);
    assert_eq!(
        fs::read_to_string(integration.path().join("src/base.txt")).expect("integration dirty"),
        "user-dirty\n"
    );

    let left = integration
        .fork_task_worktree(owned_root.join("left"), "aer/task/resource-left")
        .expect("left worktree");
    let right = integration
        .fork_task_worktree(owned_root.join("right"), "aer/task/resource-right")
        .expect("right worktree");
    assert_eq!(left.base_commit(), baseline);
    assert_eq!(right.base_commit(), baseline);

    fs::write(left.path().join("src/left.txt"), "left-changed\n").expect("left change");
    git(left.path(), &["add", "src/left.txt"]);
    git(left.path(), &["commit", "-m", "left task"]);
    fs::write(right.path().join("src/right.txt"), "right-changed\n").expect("right change");
    git(right.path(), &["add", "src/right.txt"]);
    git(right.path(), &["commit", "-m", "right task"]);

    let left_changes = left.change_set().expect("left change set");
    let right_changes = right.change_set().expect("right change set");
    let plan = IntegrationPlan::build(
        vec![
            LocalBranchEvidence {
                task_id: "left".to_owned(),
                changes: left_changes.clone(),
                semantic_write_set: SemanticWriteSet::new(["module:left"]).expect("left semantics"),
                local_verification_passed: true,
                evidence_ids: BTreeSet::from(["left-proof".to_owned()]),
            },
            LocalBranchEvidence {
                task_id: "right".to_owned(),
                changes: right_changes.clone(),
                semantic_write_set: SemanticWriteSet::new(["module:right"])
                    .expect("right semantics"),
                local_verification_passed: true,
                evidence_ids: BTreeSet::from(["right-proof".to_owned()]),
            },
        ],
        4,
    )
    .expect("integration plan");
    let mut barrier = IntegrationBarrier::new(plan);

    let left_merge = integration.merge_task(&left_changes).expect("merge left");
    barrier
        .record_merge("left", left_merge.resulting_head)
        .expect("record left");
    let right_merge = integration.merge_task(&right_changes).expect("merge right");
    barrier
        .record_merge("right", right_merge.resulting_head.clone())
        .expect("record right");
    assert_eq!(
        fs::read_to_string(integration.path().join("src/left.txt")).expect("merged left"),
        "left-changed\n"
    );
    assert_eq!(
        fs::read_to_string(integration.path().join("src/right.txt")).expect("merged right"),
        "right-changed\n"
    );
    barrier
        .finalize(IntegrationVerification {
            passed: true,
            integration_head: right_merge.resulting_head,
            proof_manifest_ids: BTreeSet::from(["integration-proof".to_owned()]),
            environment_fingerprint: "env-resource-bench".to_owned(),
            repo_snapshot: "repo-resource-bench".to_owned(),
        })
        .expect("integration acceptance");

    let managed = list_managed_worktrees(&repo, "aer/").expect("managed worktrees");
    assert_eq!(managed.len(), 3);
    assert_eq!(git(&repo, &["branch", "--show-current"]), user_branch);
    assert_eq!(
        fs::read_to_string(repo.join("src/base.txt")).expect("user dirty tracked"),
        "user-dirty\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("local-only.txt")).expect("user untracked"),
        "do-not-touch\n"
    );

    left.remove_and_delete_branch().expect("remove left");
    right.remove_and_delete_branch().expect("remove right");
    integration
        .remove_and_delete_branch()
        .expect("remove integration");
    fs::remove_dir_all(owned_root).expect("remove owned root");
    fs::remove_dir_all(repo).expect("remove repo");
}
