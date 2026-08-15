# Failure Detection and Recovery

## Objective

Detect when execution is not producing useful information and recover before an agent burns large context/token budgets repeating itself.

## Failure classes

```text
specification_error
context_missing
context_misleading
model_capability_limit
tool_failure
environment_failure
implementation_error
verification_failure
integration_failure
security_policy_block
budget_exhaustion
stagnation_loop
architecture_drift
```

Classification can be probabilistic but must remain observable.

## Progress signals

Track per task window:

- new evidence generated,
- new relevant files/symbols discovered,
- failing-test count/distance,
- accepted subgoals,
- edit-revert frequency,
- repeated commands,
- repeated file reads,
- hypothesis churn,
- context novelty,
- verifier delta,
- cost per unit progress.

## Stagnation / entropy detector

A simple initial heuristic can flag:

```text
repetition high
AND new_evidence low
AND verification_progress flat
```

Do not use a single magic threshold across all task types. Calibration is required.

## Recovery ladder

Escalate minimally:

1. **Tool-level retry** for transient infrastructure failures.
2. **Context refresh** if evidence indicates missing/stale context.
3. **Hypothesis reset**: summarize known facts, disproven paths, and produce fresh diagnostic plan.
4. **Fresh-context takeover** using Handoff ABI.
5. **Model escalation** if capability is likely limiting.
6. **Topology change**: add specialist/diagnostician or split task.
7. **Rollback / branch alternative** if current patch path is damaging.
8. **User intervention** only when semantics/authority genuinely require it.

## Fresh-context diagnostic

A diagnostician should receive:

- task objective,
- exact failures,
- accepted facts,
- disproven hypotheses,
- relevant diff,
- bounded context.

It should not inherit the complete possibly-corrupted reasoning trajectory by default.

## Reproducibility

Before expensive diagnosis, attempt to capture a reproducible failure command/environment. A non-reproducible failure may be infrastructure noise rather than code logic.

## Recovery budget

Recovery consumes a separate budget. Repeated escalations without improved evidence eventually stop the task as unresolved rather than running forever.

## No silent success

If a failure disappears because a test was removed, skipped, weakened, or environment changed, verification integrity checks must prevent that from being interpreted as recovery.
