# Handoff ABI and Cognitive Adapters

## Objective

Transfer engineering state between models/agents without transcript bloat, semantic drift, or provider lock-in.

## Rule

**Models do not communicate by unstructured free-form chat as the authoritative channel.**

They exchange typed task/evidence/state artifacts. Natural language is rendered at the model boundary by a Cognitive Adapter.

## Handoff Envelope

Minimum fields:

```text
handoff_id
task_id
objective
spec_version
repo_snapshot
requirements[]
constraints[]
invariants[]
current_state
known_facts[]
hypotheses[]
attempts[]
failures[]
evidence_refs[]
relevant_context_refs[]
unresolved_dependencies[]
requested_action
budget
expected_output_contract
```

See `schemas/handoff.schema.json`.

## Semantic categories

### KnownFact

Must have provenance and confidence. Production-authoritative facts require evidence.

### Hypothesis

May be unverified. Has state:

```text
open | supported | disproven | superseded
```

### Attempt

Records action plus outcome, not a narrative of the entire reasoning process.

### Failure

Contains reproducible symptom/evidence and classification.

### Decision

Contains chosen alternative, rationale and authority source.

## Model-specific Cognitive Adapter

The adapter converts the same Handoff Envelope into the best request form for a target model.

It may control:

- instruction ordering,
- tool presentation,
- XML/Markdown/JSON framing,
- context grouping,
- output schema wording,
- effort/reasoning setting,
- cache-friendly stable prefix.

The adapter MUST NOT change task semantics.

## Prompt optimization

Adapter templates are versioned policies. They may be optimized empirically, but every candidate must be evaluated on held-out tasks.

The goal is not to discover a mystical “language models speak.” The goal is to **compile a stable semantic contract into a model-efficient representation**.

## Handoff compression

A handoff should contain what the next worker needs to continue, especially:

- accomplished state,
- evidence,
- blockers,
- disproven paths,
- exact next action.

It should omit:

- duplicated discussion,
- motivational prose,
- irrelevant model reasoning,
- tool output already represented by an evidence hash.

## Takeover/resume

When a fresh context/model takes over a task, it receives:

1. Task Envelope,
2. latest verified Engineering State projection,
3. unresolved hypotheses/failures,
4. bounded Context Pack,
5. latest relevant diff/evidence.

This is the primary long-horizon continuity mechanism.

## Structured result

Workers return a typed `WorkResult`:

```text
status
summary
changes[]
new_facts[]
new_hypotheses[]
evidence_refs[]
blocked_on[]
recommended_next_actions[]
claimed_requirement_coverage[]
```

Claims remain untrusted until verification.
