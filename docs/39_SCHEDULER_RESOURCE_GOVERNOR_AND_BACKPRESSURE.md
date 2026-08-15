# Scheduler, Resource Governor, and Backpressure

## 1. Objective

Dynamic orchestration is safe only if resource demand is bounded.

AER MUST never translate “more independent tasks exist” into unbounded workers, model requests, subprocesses, logs, memory, disk, or network.

ADR-0007 makes bounded admission control a core invariant.

## 2. Resource vector

Each task/attempt declares or receives estimates for:

```text
cpu
memory
disk
pids/processes
open_files
wall_time
network_bytes
local_ports
provider_requests
provider_input_tokens
provider_output_tokens
provider_concurrency
monetary_cost
gpu/vram when applicable
```

Unknown estimates are represented as uncertainty, not zero.

## 3. Admission control

Before an attempt becomes `running`, the Resource Governor checks:

- global hard limits,
- project/run budgets,
- sandbox capacity,
- provider quotas,
- worker count,
- disk/temp capacity,
- dependency/service reservations,
- serialization policy.

If capacity is unavailable, the task remains ready/queued with a reason. It does not spawn and hope.

## 4. Bounded queues

Every internal queue/channel has:

- explicit capacity,
- overflow/backpressure behavior,
- producer ownership,
- consumer ownership,
- telemetry.

No unbounded event/model/tool/output queue is allowed in the daemon.

For high-volume logs and stream deltas:

- durable important events are never silently dropped;
- non-authoritative presentation deltas MAY be coalesced;
- large payloads spill to the object store rather than RAM.

## 5. Scheduling objective

The scheduler combines task value with scarce-resource opportunity cost.

Initial priorities may include:

```text
critical_path
unblock_value
risk_reduction
information_gain
user_waiting
age/fairness
resource_fit
estimated_cost
merge_conflict_risk
```

Priority MUST NOT permit starvation.

## 6. Fairness

When multiple projects/runs are active, use weighted fairness rather than one large project consuming all slots.

Organization/user policy MAY reserve:

- interactive capacity,
- verifier capacity,
- recovery capacity,
- provider quota headroom.

Verification SHOULD not be starved by generators.

## 7. Leases, heartbeats, and ownership

A running task has one active lease.

Workers heartbeat through the coordinator.

On missed heartbeat:

```text
healthy -> suspect -> expired/recoverable
```

Do not instantly duplicate a possibly active external side effect.

Recovery first classifies whether the attempt was:

- pure/model-only,
- workspace-local,
- externally mutating.

## 8. Cancellation

Cancellation is a protocol, not process murder.

Sequence:

1. mark cancellation requested;
2. stop admitting new child/tool actions;
3. request provider/tool cancellation;
4. allow bounded cleanup/evidence flush;
5. terminate sandbox process tree if deadline expires;
6. persist final attempt state.

Externally mutating operations may require reconciliation before the task can be safely retried.

## 9. Preemption

Preemption MAY be used for low-risk compute-heavy work when scarce interactive/critical work arrives.

Only preempt at safe boundaries when state can be checkpointed or discarded without semantic ambiguity.

Do not preempt migration/external-write critical sections arbitrarily.

## 10. Provider-aware scheduling

The Resource Governor integrates with `37`:

- reserve request/token capacity,
- honor reset hints,
- pace high-token calls,
- avoid retry thundering herd,
- distribute eligible requests across endpoints only when routing/privacy policy permits.

## 11. Service lifecycle

Ephemeral databases, browsers, dev servers and containers are resources owned by a task/run.

They require:

```text
owner
port/resource reservation
health check
shutdown policy
log/evidence path
cleanup deadline
```

Orphan detection runs after crashes.

## 12. Deadlock/livelock protection

Detect:

- cyclic task dependencies,
- mutually waiting resource reservations,
- tasks blocked on services owned by cancelled attempts,
- provider quota waits with no future reset information,
- repeated acquire/release loops.

Recovery must surface the concrete dependency/resource edge.

## 13. Resource telemetry

Track:

- queue wait time,
- resource utilization,
- reservation accuracy,
- worker idle time,
- provider quota saturation,
- cancelled/preempted work,
- orphan cleanup,
- starvation age,
- parallel speedup vs coordination cost.

## 14. Property tests

Generate adversarial schedules and assert:

- hard budgets never exceed policy;
- worker count is bounded;
- only one active task lease exists;
- cancellation eventually reaches terminal state;
- verification retains capacity under load;
- no queue grows without bound;
- crash/restart does not duplicate leases or external writes silently.
