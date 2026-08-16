# Engineering State and Memory

## Goal

Preserve exactly the information needed for reliable long-horizon development without turning an ever-growing chat history into pseudo-memory.

## Principle

AER has **engineering state**, not one undifferentiated memory store.

Repository Intelligence 2.0 (`06`) provides the repository-side temporal graph that memory can reference. Memory is not a second code index: it stores durable engineering conclusions whose validity is explicitly connected to source/evidence and invalidation scopes.

## State categories

### Verified Fact

An evidence-backed claim.

```text
fact_id
statement
scope
provenance/evidence_refs
repository_entity_refs[]
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

A fingerprint SHOULD link to the affected repository entities, exact failure evidence and the code/environment scope under which it was observed.

### Progress State

Accepted tasks, current blockers, pending work, current repo/spec identities.

## Authority

Embeddings/vector similarity may retrieve memory, but similarity does not establish truth.

The authoritative state is structured records backed by the event journal and evidence.

Repository graph edges follow the same rule: a graph makes relationships queryable, but only its provenance determines whether a relationship is extracted, semantically resolved, observed or inferred.

## Repository-linked memory

Durable repository memory should behave more like an evidence-governed knowledge network than a chat transcript.

A memory record MAY link bidirectionally to:

- files/symbols/modules/packages;
- requirements and ADRs;
- build/test targets;
- commits/renames;
- runtime observations;
- failures and proof evidence;
- other facts, decisions and hypotheses.

Useful relations include:

```text
about
supported_by
contradicted_by
supersedes
invalidated_by
observed_in
failed_at
implements_requirement
relevant_to
```

Backlinks SHOULD be derived automatically from these relations so an agent can cheaply ask which facts/decisions/failures concern one repository entity.

This borrows the useful navigation model of linked-note systems such as Obsidian while retaining AER's structured store as authority. A Markdown/Obsidian-compatible knowledge notebook MAY be generated for human inspection, but it is a view, not the canonical state.

## Temporal validity

Long-lived repository knowledge MUST model revision, not only accumulation.

Every important record should be in one of these validity states:

```text
current
potentially_stale
invalidated
superseded
```

A source change does not automatically delete history. Instead AER records why the previous statement stopped being current and retains its evidence for audit/replay.

Examples:

- a verified fact about `AuthService` becomes `potentially_stale` when its implementation/dependencies change;
- a compiler-resolved call relation is invalidated when its semantic-index scope changes;
- a failure fingerprint remains historically valid but is no longer offered as current guidance after the relevant subsystem is rewritten;
- a user architecture decision survives source edits unless explicitly superseded, but its implementation links may need re-resolution.

Repository Intelligence supplies change/impact information; the memory layer applies the record's declared invalidation policy.

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

Negative memory SHOULD also contain repository entity references. If a major rewrite invalidates the old investigation scope, the Context Engine can stop applying that fingerprint automatically.

## Compaction

Raw trajectories can be enormous. Compact them into structured state when:

- context pressure increases;
- worker handoff occurs;
- a task reaches a semantic checkpoint;
- a run ends.

Compaction output should retain:

- accepted facts;
- open blockers;
- failed/disproven paths;
- decisions;
- exact evidence refs;
- repository entity refs and source hashes;
- current diff/task state.

Compaction MUST NOT manufacture repository semantics that were absent from the underlying evidence. In particular, a summary cannot upgrade a heuristic Repository Intelligence edge into a semantically resolved relationship.

## Source preservation

Do not delete the underlying raw artifact merely because a summary exists. Store the summary plus source hashes so fidelity can be audited.

For repository-linked memory, preserve enough source/evidence identity to determine whether a fact is still applicable after later commits.

## Invalidation

Facts can become stale after:

- spec changes;
- relevant file/symbol/module changes;
- dependency or lockfile upgrades;
- build topology changes;
- environment changes;
- semantic-adapter/parser version changes where the fact depends on them;
- new contradictory evidence.

Records MUST carry invalidation scope.

Repository Intelligence 2.0 SHOULD provide an incremental invalidation frontier: code/build changes identify the smallest affected graph neighborhood, then memory records referencing that neighborhood are reclassified without rescanning unrelated memory.

## Retrieval

Memory retrieval is task-conditioned and uses the Context Engine. Do not inject all prior facts into all model calls.

Prefer a progressive retrieval sequence:

1. entity IDs + short statements + validity state;
2. backlinks and supporting/contradicting relations;
3. evidence/source anchors;
4. historical trajectory only when needed.

This keeps repository memory useful without paying transcript-scale token cost.

## Memory quality telemetry

Measure:

- rediscovery avoided;
- tokens/tool calls saved after handoff;
- stale-memory selection rate;
- invalidation precision/recall;
- contradiction detection;
- repeated-dead-end rate;
- percentage of injected memory actually used;
- downstream verified success with/without memory.

More stored memory is not automatically better memory.

## Cross-project learning

User/project secrets and source code must not automatically become global reusable memory.

Cross-project experience should preferentially store abstract policy statistics such as:

- model X succeeded on task class Y;
- retrieval policy Z reduced re-exploration;
- verifier composition caught regressions.

Any reusable content must respect privacy and tenancy policy.
