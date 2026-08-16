# Evaluation and Benchmark Strategy

## Goal

Measure the entire model+harness system under realistic, long-horizon conditions. AER is not improved because a model says it is better or one public benchmark score rises.

## Evaluation hierarchy

### E0 — deterministic unit/contract/property tests

For:

- state machines and schema semantics;
- resource admission/backpressure;
- provider normalization/retry safety;
- storage/migrations;
- sandbox and VCS ownership;
- policy precedence;
- parsers and compatibility handshakes.

### E1 — component evals

- intent extraction / ambiguity detection;
- research claim/citation/freshness quality;
- repository retrieval / Context Pack quality;
- repository graph/semantic-index correctness and freshness;
- routing decisions;
- provider resilience;
- handoff fidelity;
- verifier precision/recall;
- domain-profile composition.

### E2 — short engineering tasks

Bug fixes/refactors/features with strong verifiers. Fast feedback, but insufficient for product claims.

### E3 — long-horizon project tasks

Multi-file, multi-stage, evolving or from-scratch work measuring continuity, resource economics and architecture health.

### E4 — continuous project evolution

Sequences of dependent feature requests on the same codebase exposing structural erosion, dependency drift and accumulated bad decisions.

### E5 — adversarial integrity/security/supply-chain

- reward hacking/test tampering;
- prompt/research poisoning;
- malicious repo/package content;
- credential exfiltration;
- sandbox boundaries;
- provider partial-response/retry faults;
- malicious/rollback update metadata;
- migration crash injection.

## Public benchmark use

Use public datasets as one input for repository issue resolution, long-horizon roadmap/version tasks, iterative code evolution, specification reasoning, context retrieval and model routing.

Repository-exploration evaluation SHOULD include line-budgeted and multilingual tasks in the style of current repository-localization research rather than measuring only final patch success.

Maintain private held-out suites to reduce contamination and benchmark-shaped optimization.

## AER-native benchmark families

### IntentBench

Score:

- material unknowns found;
- unnecessary questions;
- requirement fidelity;
- semantic-checksum errors.

### ResearchBench

Given time-sensitive/contested technical questions, score:

- primary/direct source selection;
- claim citation coverage/entailment;
- contradiction discovery;
- temporal correctness/freshness;
- poisoned-source resistance;
- unsupported promotion into Engineering IR.

### ContextBench

Score:

- relevant file/symbol/requirement recall;
- budgeted context yield;
- token cost;
- post-seed re-exploration;
- stale/misleading selection.

### RepoIntelBench

Repository Intelligence 2.0 (`06`) requires a dedicated component suite. A graph visualization or high parser count is not evidence of quality.

#### Language/capability coverage

Record per repository and language:

```text
recognized_source_files
recognized_source_bytes
Tier0_lexical_coverage
Tier1_syntax_coverage
Tier2_project_resolution_coverage
Tier3_precise_semantic_coverage
fallback_rate
ambiguous_language_rate
generated_vendor_classification_accuracy
```

Do not report one “number of supported languages” without the capability distribution.

#### Structural/semantic correctness

Use hand-checked and tool-derived fixtures for:

- declarations/scopes;
- imports/exports/re-exports;
- definitions/references;
- calls and resolved calls;
- inheritance/implementations;
- package/build dependencies;
- generated-source relationships;
- test associations;
- source ranges;
- provenance class.

Report precision/recall by edge type and producer. A syntax-derived candidate edge and a compiler-resolved edge are scored separately.

#### Freshness/incrementality

Mutation sequences MUST exercise:

- edit inside one function;
- file add/delete;
- rename/move;
- module/package rename;
- lockfile/dependency change;
- build-target change;
- parser/query/semantic-adapter version change;
- dirty worktree;
- unavailable semantic adapter.

Measure stale-edge incidents and verify that unaffected content-addressed artifacts are reused.

#### Performance

Measure distributions, not one best-case number:

```text
cold_index_files_per_second
cold_index_LOC_or_bytes_per_second
warm_reuse_ratio
incremental_update_p50/p95
time_to_first_queryable_snapshot
graph_query_p50/p95
exact_symbol_lookup_p50/p95
impact_query_p50/p95
peak_RSS
persistent_bytes_per_LOC_or_symbol
semantic_adapter_startup_and_amortized_cost
```

Use small, medium, large and monorepo-scale fixtures. Include Windows and Linux because parser/process/filesystem behavior can differ.

#### Retrieval value

Compare at minimum:

1. lexical-only baseline;
2. current AER Repository Intelligence baseline;
3. graph-only retrieval;
4. embedding-only retrieval;
5. Repository Intelligence 2.0 hybrid retrieval.

Measure:

- Hit@K/MRR for relevant files;
- relevant symbol/line recall;
- relevant lines per 1K context tokens;
- exploration tool calls/tokens;
- time to first relevant source;
- abstention calibration;
- final verified task success.

The hybrid design is accepted only if its additional complexity earns measurable retrieval or engineering value.

#### Repository-memory value

For repeated work on the same repository, compare fresh-start versus repository-memory-assisted runs. Measure rediscovery, stale-memory selection, invalidation accuracy, repeated dead ends, token cost and verified success.

#### Adversarial repository fixtures

Include:

- syntax errors/partially typed code;
- ambiguous extensions;
- generated and vendored code;
- symlinks/path edge cases;
- huge files;
- submodules;
- cyclic dependencies;
- mixed-language services;
- code generation;
- missing build tools;
- broken or stale LSP/SCIP/compiler indexes;
- intentionally misleading inferred edges.

### ProviderBench

Fault-inject:

- 429/rate limits;
- transient/server errors;
- malformed structured outputs;
- partial/truncated streams;
- duplicate/replayed tool calls;
- disconnect/cancellation;
- pricing/model-alias drift.

Score correctness, bounded retry, duplicate-side-effect avoidance, cost/accounting and recovery.

### RouterBench

Execute representative tasks across eligible models; measure verified success, cost, latency and regret.

### HandoffBench

Interrupt/resume with new model/context; measure time/tokens to productive action, rediscovery, semantic loss and final success.

### ResourceBench

Generate adversarial task graphs/provider limits/local resource pressure; measure:

- bounded queues/workers;
- admission accuracy;
- fairness/starvation;
- verifier capacity;
- cancellation/recovery;
- parallel benefit vs coordination/resource cost.

### ProofBench

Measure evidence/Proof Manifest correctness, verifier tampering resistance, stale evidence invalidation and requirement coverage.

### DomainBench

Representative web/backend/CLI/systems/ML/IaC tasks validate domain profile selection and required non-functional evidence.

### EvolutionBench

Repeated feature changes measure checkpoint pass rate, architecture-health trajectory, verbosity/complexity growth and later change cost.

### MigrationBench

Maintain fixture states from supported historical versions. Inject crashes at migration boundaries and test:

- successful upgrades;
- postcondition/replay equivalence;
- unsupported downgrade refusal;
- CLI/daemon compatibility negotiation;
- restore/recovery paths.

### SupplyChainBench

Fixtures cover malicious package hooks, dependency confusion/integrity changes, lockfile drift, stale advisories, provenance/signature verification and update rollback/freeze scenarios.

### IntegrityBench

Broad security/adversarial suite spanning agent authority, prompt injection, research poisoning, verifier gaming, release/update trust and state poisoning.

## Reproducibility

Every eval run records relevant:

- AER binary/API/schema versions;
- model/provider snapshot;
- policy versions;
- Engineering IR version;
- repository/workspace snapshot;
- Repository Intelligence index/schema/parser/adapter versions when repository retrieval is evaluated;
- Environment Fingerprint;
- dependency lock/tool versions;
- sandbox image/policy;
- Domain Profile versions;
- Research Artifact/source snapshots where external facts matter;
- random seeds where meaningful;
- pricing snapshot.

## Infrastructure noise

Classify/retry only clearly transient infrastructure failures. Do not silently retry semantic failures until one passes.

ProviderBench and environment fingerprints help separate runtime noise from engineering failure.

## Statistics

Policy promotion uses repeated tasks and uncertainty estimates.

Report distributions/confidence intervals and task-stratified outcomes, not only averages.

For multi-objective changes, report verified quality, cost, latency, security/integrity and architecture-health tradeoffs.

## Counterfactual honesty

Counterfactual outcomes are facts only when actually replayed/executed under comparable conditions.

Do not label model predictions of “what another model would have done” as RouterBench ground truth.

## Human review

Periodically sample accepted/rejected runs, research artifacts, repository-index errors, migration failures and policy promotions to discover evaluator blind spots.

Human review is calibration data, not the default runtime bottleneck.
