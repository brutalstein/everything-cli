# Engineering State and Memory

## Goal

Preserve exactly the information needed for reliable long-horizon development without turning an ever-growing chat history into pseudo-memory.

## Principle

AER has **engineering state**, not one undifferentiated memory store.

## State categories

### Verified Fact

An evidence-backed claim.

```text
fact_id
statement
scope
provenance/evidence_refs
confidence
created_at
valid_for_repo_snapshot/spec_version
expiry/invalidation rule
```

### User Decision

A decision explicitly attributable to the user.

### System Decision

An architecture/implementation choice made by AER with rationale.

### Assumption

A provisional statement used to continue work. Must never masquerade as fact.

### Hypothesis

A testable explanation under investigation.

States:

```text
open -> supported | disproven | superseded
```

### Failure Fingerprint

Reusable representation of a failed approach or recurrent failure signature.

### Progress State

Accepted tasks, current blockers, pending work, current repo/spec identities.

## Authority

Embeddings/vector similarity may retrieve memory, but similarity does not establish truth.

The authoritative state is structured records backed by the event journal and evidence.

## Negative memory / anti-loop state

Disproven hypotheses are valuable.

Example:

```yaml
hypothesis: "Duplicate messages are caused by Redis pub/sub replay"
status: disproven
evidence:
  - trace: ...
  - controlled_test: ...
revisit_if:
  - "Redis topology changes"
```

This prevents repeated expensive dead ends after context resets.

## Compaction

Raw trajectories can be enormous. Compact them into structured state when:

- context pressure increases,
- worker handoff occurs,
- a task reaches a semantic checkpoint,
- a run ends.

Compaction output should retain:

- accepted facts,
- open blockers,
- failed/disproven paths,
- decisions,
- exact evidence refs,
- current diff/task state.

## Source preservation

Do not delete the underlying raw artifact merely because a summary exists. Store the summary plus source hashes so fidelity can be audited.

## Invalidation

Facts can become stale after:

- spec changes,
- relevant file changes,
- dependency upgrades,
- environment changes,
- new contradictory evidence.

Records MUST carry invalidation scope.

## Retrieval

Memory retrieval is task-conditioned and uses the Context Engine. Do not inject all prior facts into all model calls.

## Cross-project learning

User/project secrets and source code must not automatically become global reusable memory.

Cross-project experience should preferentially store abstract policy statistics such as:

- model X succeeded on task class Y,
- retrieval policy Z reduced re-exploration,
- verifier composition caught regressions.

Any reusable content must respect privacy and tenancy policy.
