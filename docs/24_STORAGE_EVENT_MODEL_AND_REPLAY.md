# Storage, Event Model, and Replay

## Design

Use **SQLite + append-only event journal + content-addressed object store** for the initial local runtime.

This is intentionally simpler than introducing a distributed database, message broker, graph database, and vector service before they are necessary.

## Why SQLite

For a local coordinator it provides:

- transactional durability,
- WAL mode,
- mature cross-platform support,
- inspectability,
- low operational overhead,
- sufficient relational modeling for task/state metadata.

Scale-out interfaces should exist, but v1 should earn the need for distributed storage.

## Core tables / views

Illustrative:

```text
projects
spec_versions
requirements
tasks
task_dependencies
runs
attempts
model_calls
tool_calls
evidence
facts
hypotheses
decisions
context_packs
policy_versions
model_capability_stats
architecture_health
artifacts
events
```

## Event journal

Every material event has:

```text
event_id (monotonic/ULID)
project_id
run_id
task_id (nullable)
event_type
schema_version
timestamp
payload_json or payload_artifact_hash
causation_id
correlation_id
```

## Event examples

```text
project.created
spec.compiled
spec.delta_applied
task.created
task.ready
task.lease_acquired
routing.decided
context.pack_created
model.call_started
model.call_completed
tool.call_completed
worktree.diff_recorded
evidence.created
verification.verdict
task.accepted
task.rejected
policy.proposed
```

## Materialized state

State tables are projections. If feasible, critical projections should be rebuildable from event history and immutable artifacts.

Not every byte of stdout belongs inline in events; store large content by hash.

## Content-addressed objects

Object identity:

```text
sha256(content)
```

Objects include:

- raw tool output,
- patches,
- context materializations,
- screenshots,
- test reports,
- trace bundles,
- generated summaries,
- evaluator reports.

Metadata records sensitivity and retention policy.

## Crash consistency

Use transactional outbox-like ordering for external effects where needed.

For task execution:

1. record intended task lease/attempt;
2. create sandbox/worktree;
3. execute;
4. persist result artifacts;
5. journal completion;
6. transition task projection.

On restart, incomplete attempts are recovered explicitly.

## Replay

Replay modes:

### State replay

Rebuild projections from events/artifacts.

### Decision replay

Re-run deterministic policy decisions against historic state.

### Agent replay

Re-execute model/tool work in a reconstructed environment. More expensive and not guaranteed bit-identical.

## Schema migrations

All DB and event schema migrations are versioned. Event payload schemas should prefer additive evolution. Destructive reinterpretation of historical events is prohibited.
