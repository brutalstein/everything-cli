from pathlib import Path

path = Path("STATUS.md")
text = path.read_text(encoding="utf-8")

old_header = '''**Current phase:** Phase 6 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery  
**Current step:** 12 / 18 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-12 code HEAD:** `5446fb2d4887d0bc7b18174af6cc81bd349a837c`  
**Verified Step-12 CI:** `foundation-ci` run `31957740270` — Ubuntu PASS including RepoIntelBench, Tier-2 topology and HandoffBench; canonical isolated Windows verifier PASS  
**Next step:** 13 — Bounded Parallel Execution — BLOCKED until Step-12 target Windows verification passes
'''
new_header = '''**Current phase:** Phase 7 — Bounded Parallel Execution  
**Current step:** 13 / 18 — Bounded Parallel Execution  
**Repository-side state:** IMPLEMENTED — authoritative Linux/Windows CI pending  
**Step-13 branch:** `agent/step-13-bounded-parallel-execution`  
**Step-13 focused verification:** PASS on Ubuntu during implementation; permanent ResourceBench + canonical Windows gates added  
**Next step:** 14 — Architecture Health Controller — BLOCKED until Step-13 target Windows verification passes
'''
if text.count(old_header) != 1:
    raise SystemExit(f"expected one status header, found {text.count(old_header)}")
text = text.replace(old_header, new_header, 1)

text = text.replace(
    '`crates/aer-cli/**` was not modified by Steps 10–12.',
    '`crates/aer-cli/**` was not modified by Steps 10–13.',
    1,
)

milestone_anchor = '- **Step 11 — Provider Resilience + Cost Router:** COMPLETE — repository CI `31951923261`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.\n'
if text.count(milestone_anchor) != 1:
    raise SystemExit("Step 11 milestone anchor missing")
text = text.replace(
    milestone_anchor,
    milestone_anchor
    + '- **Step 12 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery:** COMPLETE — repository CI `31957740270`; post-merge main CI `31958367494`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.\n',
    1,
)

if text.count('**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING') != 1:
    raise SystemExit("Step 12 pending state marker missing or ambiguous")
text = text.replace(
    '**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING',
    '**State:** COMPLETE',
    1,
)

pending_row = '| Target Windows canonical verifier | PENDING | user reproduction required after Step 12 is merged to `main`. |'
pass_row = '| Target Windows canonical verifier | PASS | user reproduction on 2026-08-16; final line `everything Windows verification: PASS`. |'
if text.count(pending_row) != 1:
    raise SystemExit("Step 12 target-Windows row missing")
text = text.replace(pending_row, pass_row, 1)

exit_marker = '## Step 12 exit condition\n\n'
if text.count(exit_marker) != 1:
    raise SystemExit("Step 12 exit marker missing")
prefix = text.split(exit_marker, 1)[0]
new_tail = r'''## Step 12 exit condition

Step 12 is closed. The target Windows checkout reproduced the canonical verifier on 2026-08-16; its acceptance evidence remains above for audit/replay.

## Step 13 — Bounded Parallel Execution

**State:** IMPLEMENTED — AUTHORITATIVE REPOSITORY CI PENDING

### Ownership and scope

Step 13 adds bounded parallel execution without replacing the authorities established in earlier phases:

- `aer-domain::scheduling` owns deterministic dependency/fairness/conflict policy;
- the existing `RuntimeSafetyKernel`, `ResourceGovernor`, leases and cancellation protocol remain authoritative for resource and attempt ownership;
- `aer-workspace::parallel` owns branch-backed isolated worktrees and integration Git operations;
- `aer-core::parallel` coordinates those authorities and owns the integration barrier, measured utility policy and bounded orphan registry.

There are no fixed Planner/Coder/Reviewer/Tester personas. Parallelism remains a conditional optimization over independently verifiable work intents, and the CLI/TUI remains frozen.

### Dependency-aware bounded scheduler

`TaskGraph` validates duplicate/self/unknown dependencies and rejects cycles before scheduling. Dependency-satisfied pending tasks can become ready through the existing `TaskState` transition rules.

`BoundedScheduler` applies a hard ready-set bound and hard active-task bound. Candidate order is deterministic and combines weighted per-run service accounting with the documented priority signals: unblock value, critical path, risk reduction, information gain, user waiting, age, expected cost and merge-conflict risk.

Run weights provide deterministic weighted fairness; service accounting prevents one run from consuming every parallel slot merely because it has more ready tasks. Parallelism is denied or serialized when contracts are unstable, local verification is unavailable, resource demand is unknown, a task is explicitly serial-only, high-risk serialization applies, or predicted write/semantic ownership overlaps active work.

### Predicted and observed conflict control

`WriteScope` stores normalized repository-relative predicted/observed write prefixes and rejects host-absolute/path-escape scopes. Prefix overlap is component-aware, so `src/auth` overlaps `src/auth/token.rs` but not `src/authz`.

`SemanticWriteSet` separately represents shared API/schema/requirement-like ownership keys. The scheduler blocks predicted textual and semantic overlap before admission, while runtime observations can detect unpredicted actual write expansion before integration.

### Resource, lease, cancellation and preemption coordination

`ParallelRuntimeCoordinator` does not duplicate resource or lease truth. Scheduler bookkeeping is coupled to authoritative `RuntimeSafetyKernel::start_task`; a denied resource/lease admission rolls scheduler ownership back rather than leaving a ghost active task.

Existing verifier reservation therefore remains effective under generator load. Verification/cancellation finalization releases authoritative runtime ownership before scheduler ownership is removed.

Preemption is conservative: only lower-priority generator work explicitly marked discardable/checkpointable can be selected. External-mutating attempts and `PreemptionSafety::Never` work are excluded.

### Isolated branch-backed worktrees

A captured exact `WorkspaceSnapshot` is first materialized into an AER-owned integration worktree. Dirty tracked/untracked user state is captured into an internal deterministic baseline commit on the integration branch; the user's branch and working tree are never changed.

Every writable parallel task receives a dedicated branch/worktree forked from the same integration baseline for its wave. Task change sets record base/head identity, actual changed paths and dirty state.

Integration merge commits use AER-owned deterministic Git author/committer identity rather than depending on the user's global Git identity. Worktree discovery is bounded through Git's porcelain worktree inventory and branch-prefix ownership.

### Integration barrier

Branch-local verification is necessary but insufficient. `LocalBranchEvidence` requires a clean committed change plus local evidence. `IntegrationPlan` requires one common base, bounded candidate count and no duplicate task evidence; exact changed-path overlap and semantic ownership overlap fail before merge.

Branches are merged into the isolated integration worktree in deterministic task order. `IntegrationBarrier` refuses acceptance until all planned merges are recorded and an integration-aware verification result is bound to the exact final integration head, repository snapshot, environment fingerprint and proof-manifest identities.

### Bounded orphan cleanup

`OrphanRegistry` records worktrees, process trees, ephemeral services, local ports and containers with task ownership and cleanup deadlines. Registry size and cleanup-per-sweep are hard-bounded. Live owners are never swept; failed cleanup remains registered for recovery; duplicate registration fails before mutation so ownership authority cannot be overwritten by an error path.

### Measured parallel utility

`ParallelUtilityMeasurement` compares serial wall time, parallel wall time plus coordination overhead, verified-success count and monetary cost under an explicit `ParallelUtilityPolicy`. Parallelism is not assumed to be useful merely because two slots exist.

ResourceBench includes a real wall-clock control using two independent bounded sleep workloads: it measures the same work serially and concurrently and requires the parallel measurement to satisfy the utility policy. This is deliberately deterministic/tool-only and has no paid provider dependency.

### ResourceBench

The Step-13 ResourceBench verifies:

- generator saturation cannot consume the reserved verifier slot;
- denied authoritative resource admission rolls back scheduler ownership;
- ready-set and active-worker counts remain bounded;
- weighted fairness surfaces work from competing runs;
- predicted-disjoint tasks that expand into the same actual write scope are detected;
- external mutation/non-preemptible work cannot be chosen for preemption;
- orphan cleanup is bounded, live-owner-safe and retry-preserving;
- duplicate orphan registration cannot overwrite authority;
- disjoint files with conflicting semantic ownership are rejected;
- branch-local PASS cannot bypass integration verification/proof;
- real dirty-user-state snapshots can fork two isolated task worktrees, merge verified branches, and leave the user's branch/tree unchanged;
- measured parallel wall-clock utility is positive against its serial control.

### Permanent Step-13 gates

Linux CI contains:

```text
cargo +1.97.1 test --locked -p aer-core --test resource_bench
```

The canonical Windows verifier runs the equivalent target-specific ResourceBench command in addition to the full workspace suite.

## Step 13 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Dependency graph validates unknown/self/cyclic dependencies | PASS | `aer-domain::scheduling::TaskGraph` tests. |
| Hard ready-set and active-worker bounds | PASS | `SchedulerPolicy` + ResourceBench. |
| Deterministic weighted run fairness | PASS | service/weight ordering + ResourceBench competing-run fixture. |
| Unknown resource demand fails closed | PASS | scheduler block reason + existing Resource Governor invariant. |
| Verifier reservation survives generator saturation | PASS | authoritative Resource Governor + ResourceBench. |
| Denied runtime admission rolls scheduler ownership back | PASS | `ParallelRuntimeCoordinator::admit_task` + ResourceBench. |
| Predicted repository write overlap blocks parallelism | PASS | component-aware `WriteScope`. |
| Semantic ownership overlap blocks parallelism | PASS | `SemanticWriteSet` + scheduler/integration tests. |
| Actual runtime write expansion is detected | PASS | observed write-scope ResourceBench fixture. |
| Exact dirty user state becomes isolated internal baseline | PASS | real Git worktree ResourceBench fixture. |
| One branch/worktree per writable parallel task | PASS | `aer-workspace::parallel`. |
| User active branch/tree not mutated by parallel workers | PASS | real worktree ResourceBench assertions. |
| Branch-local verification is not final acceptance | PASS | `LocalBranchEvidence` + `IntegrationBarrier`. |
| Exact final integration head is proof-bound | PASS | `IntegrationVerification`/`IntegratedAcceptance`. |
| Safe preemption excludes external mutation/non-preemptible attempts | PASS | scheduler candidate rules + ResourceBench. |
| Cancellation uses existing bounded protocol | PASS | `RuntimeSafetyKernel` coordination. |
| Orphan registry and cleanup are bounded | PASS | hard record/sweep limits + ResourceBench. |
| Failed orphan cleanup remains recoverable | PASS | ResourceBench retry fixture. |
| Duplicate orphan registration is mutation-free on failure | PASS | fail-before-insert rule + ResourceBench. |
| Parallel utility is measured, not assumed | PASS | serial/parallel wall-clock ResourceBench control. |
| No live/paid provider dependency in correctness gates | PASS | deterministic local fixtures only. |
| CLI/TUI freeze preserved | PASS | no `crates/aer-cli/**` Step-13 changes. |
| Focused Ubuntu format + `-D warnings` + domain/workspace/core regression | PASS | temporary read-only implementation gate before permanent CI. |
| Permanent Linux ResourceBench gate | PENDING | authoritative PR CI required. |
| Canonical isolated Windows verifier including ResourceBench | PENDING | authoritative PR CI required. |
| Full workspace regression suite | PENDING | authoritative PR CI required. |
| Temporary Step-13 workflow scaffolding removed | PENDING | remove before PR authoritative code HEAD. |
| Target Windows canonical verifier | PENDING | user reproduction required after Step 13 is merged to `main`. |

## Step 13 exit condition

Do **not** mark Step 13 complete or start Step 14 until the final production tree has passed authoritative Linux + canonical Windows CI, has been merged to `main`, and the target Windows checkout has reproduced `scripts/verify-windows.ps1` successfully.
'''
path.write_text(prefix + new_tail, encoding="utf-8")
