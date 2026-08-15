# Evaluation and Benchmark Strategy

## Goal

Measure the entire model+harness system under realistic, long-horizon conditions. AER is not considered improved because one model says it is better or because a single public benchmark score rises.

## Evaluation hierarchy

### E0 — deterministic unit/contract tests

For AER's own state machines, schemas, storage, routing filters, sandbox policy, and parsers.

### E1 — component evals

- intent extraction accuracy,
- ambiguity detection,
- repository retrieval,
- context pack quality,
- routing decisions,
- handoff fidelity,
- verifier precision/recall.

### E2 — short engineering tasks

Bug fixes/refactors with strong verifiers. Useful for fast iteration but insufficient for product claims.

### E3 — long-horizon project tasks

Multi-file, multi-stage, evolving or from-scratch tasks measuring continuity and architecture health.

### E4 — continuous project evolution

A sequence of dependent feature requests on the same codebase to expose structural erosion and accumulated bad decisions.

### E5 — adversarial integrity/security

- reward hacking,
- test tampering,
- prompt injection,
- malicious repo content,
- credential exfiltration,
- sandbox boundary attempts.

## Public benchmark use

Use public datasets as one input, including research directions represented by:

- repository issue resolution,
- long-horizon roadmap/version tasks,
- iterative code evolution,
- specification reasoning,
- context retrieval,
- routing across models.

But maintain a private held-out suite to reduce contamination and benchmark-specific overfitting.

## AER-native benchmark families

### IntentBench

Prompt contains incomplete product intent. Score:

- material unknowns found,
- unnecessary questions,
- requirement fidelity,
- semantic-checksum errors.

### ContextBench

Given task + repo snapshot, score:

- relevant file/symbol recall,
- budgeted context yield,
- token cost,
- post-seed re-exploration.

### RouterBench

Same task executed across eligible models; measure:

- verified success,
- cost,
- latency,
- cumulative regret.

### HandoffBench

Interrupt a run and resume with a new model/context. Measure:

- time/tokens to productive action,
- rediscovery,
- semantic loss,
- final success.

### EvolutionBench

Repeated feature changes over one codebase. Measure:

- checkpoint pass rate,
- architecture-health trajectory,
- verbosity/complexity growth,
- later change cost.

### IntegrityBench

Verifier-gaming and security attacks.

## Reproducibility

Every eval run records:

- model/provider snapshot,
- policy versions,
- Engineering IR version,
- repository commit,
- sandbox/environment fingerprint,
- tool versions,
- random seeds where meaningful,
- pricing snapshot.

## Infrastructure noise

Agent evals are sensitive to environment failures. Classify and retry only clearly transient infrastructure failures. Do not silently retry semantic failures until one passes.

## Statistics

Policy promotion should use repeated tasks and uncertainty estimates. Do not promote based on one anecdotal success.

For cost/success comparisons, report distributions and confidence intervals, not only averages.

## Human review

Sample accepted and rejected runs periodically to detect evaluator blind spots. Human review is calibration data, not the default runtime bottleneck.
