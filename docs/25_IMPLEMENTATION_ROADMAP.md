# Implementation Roadmap

## Rule for coding agents

Do not jump to “full multi-agent self-improving platform.” Implement foundations in causal order and enforce each phase exit gate.

Routing requires outcome data. Self-evolution requires trustworthy evals. Multi-agent execution requires reliable single-agent state, resource admission and integration. Long-horizon autonomy requires sandboxing, workspace safety, reproducibility and resumability. Production-quality releases require explicit compatibility and migration behavior.

Cross-cutting docs `35`–`44` are part of this roadmap, not optional polish.

Repository Intelligence 2.0 (`06`) is a **planned uplift of the completed Phase-3/Step-08 baseline**, not a new numbered phase. It is deliberately staged when long-horizon state needs temporal repository knowledge, primarily in Phase 6. This preserves the causal sequence and avoids building a second competing index.

---

# Phase 0 — Repository Constitution, Executable Contracts, and Eval Skeleton

## Build

- Rust workspace and CI;
- target repository boundaries;
- JSON Schema package and schema-validation tooling;
- contract registry from `44`;
- deterministic test harness;
- docs/ADR/path/reference checks;
- benchmark fixture interface;
- OpenTelemetry plumbing skeleton;
- initial compatibility/version registry;
- Resource Governor interfaces and bounded-channel conventions.

## Exit gate

- cross-platform build/test on Linux + Windows; macOS if available;
- all shipped schemas meta-validate;
- shipped examples validate against their schemas;
- event/state/resource property-test scaffolds exist;
- no model provider dependency in core domain package;
- no unbounded model/tool/task queue in foundation runtime code.

---

# Phase 1 — Durable, Safe Single-Agent Runtime

## Build

- `aer-domain`, `aer-core`, `aerd`, `aer` CLI;
- SQLite event journal + object store;
- project/run/task state machines;
- model provider abstraction with one reference adapter;
- minimal Provider Gateway retry/error/cancellation semantics from `37`;
- Tool ABI;
- Resource Governor admission/leases;
- isolated worktree lifecycle;
- dirty-workspace protection from `41`;
- command execution backend;
- Environment Fingerprint baseline from `38`;
- resumable run/cancellation/crash recovery.

## Deliberately exclude

- learned router;
- parallel agents;
- vector retrieval;
- self-evolution;
- remote workers.

## Exit gate

A simple coding task can be started, interrupted, resumed, executed in an isolated workspace, verified, and accepted with event replay, without overwriting user-owned dirty state or exceeding configured worker/resource bounds.

---

# Phase 2 — Intent Engine, Research, and Engineering IR

## Build

- interactive requirement session;
- uncertainty representation;
- selective question policy;
- Engineering IR compiler;
- JSON Schema + semantic validation;
- SpecDelta/versioning;
- semantic-checksum workflow;
- bounded external Research task pipeline from `36`;
- provenance-bearing `ResearchArtifact`;
- research-to-IR promotion rules.

## Exit gate

Greenfield prompts produce stable versioned specs; omitted/distorted critical constraints are caught by semantic-checksum evals; temporally unstable research claims remain provenance/freshness-bearing and cannot silently become user requirements.

---

# Phase 3 — Repository Intelligence and Context Economy

## Build

- commit-aware file inventory;
- lexical search;
- Tree-sitter parsing;
- symbol/dependency graph;
- git/test/runtime/requirement links;
- optional embeddings;
- retrieval fusion;
- executable Context Pack;
- token budgeter;
- extractive/source-anchored compression;
- fidelity validation for decision-critical summaries.

## Exit gate

ContextBench demonstrates better relevant-context yield/token than naive whole-map and single-retriever baselines, and Context Pack references remain valid for the declared repo/spec snapshot.

This phase establishes the executable baseline. It does not claim universal semantic language support; the richer capability ladder, temporal graph and repository-memory integration are the RI2 uplift specified in `06` and scheduled below.

---

# Phase 4 — Verification, Reproducibility, and Proof System

## Build

- evidence collector/schema;
- Environment Fingerprint integration;
- clean/reproducible verifier profiles;
- verifier composition;
- immutable verification workspace;
- requirement-to-evidence mapping;
- Proof Manifest schema;
- reward-hacking/integrity checks;
- independent semantic verifier adapter;
- architecture-health baseline hooks;
- domain verification profiles from `43`.

## Exit gate

Accepted tasks are auditable from requirement to code to immutable evidence and environment identity. Integrity tests catch deliberate test weakening/verifier tampering, and representative domains have explicit verification profiles instead of generic “run tests.”

---

# Phase 5 — Provider Resilience, Observability, Cost, and Deterministic Routing

## Build

- complete Provider Gateway behavior from `37`;
- model capability registry;
- provider health/circuit state;
- rate-limit and retry governor;
- structured-output/tool-call validation;
- token/cache/cost accounting;
- task feature extraction;
- deterministic cost/quality router;
- scout-then-escalate policy;
- effort/context budget selection.

## Exit gate

Provider fault-injection tests prove bounded retry/failover without duplicate external side effects. RouterBench shows equal-or-better verified success at meaningfully lower normalized cost than “always strongest model” with no critical regression.

---

# Phase 6 — Repository Intelligence 2.0, Recovery, and Long-Horizon Engineering State

## Build

### Repository Intelligence 2.0 uplift of the existing index

- versioned language capability registry instead of one binary supported/unsupported flag;
- universal Tier-0 lexical fallback for safely readable source/text;
- pinned Tree-sitter grammar adapter/pack architecture for broad Tier-1 syntax coverage;
- package/build/test topology adapters for Tier-2 project resolution;
- compiler/LSP/SCIP ingestion adapters for Tier-3 precise semantics where reproducibly available;
- provenance classes on graph relations (`extracted`, `semantic_resolved`, `observed`, `inferred`);
- stable snapshot identity plus evidence-bearing logical symbol continuity across rename/move;
- content-addressed incremental parsing and dependency-aware invalidation frontier;
- per-view freshness/capability state;
- build/package/generated-source nodes and cross-language links;
- graph backlinks, bounded traversal and impact queries;
- lazy role-aware retrieval representations and optional embeddings;
- transactional migration of the current `aer-repo` index rather than a parallel second index.

### Long-horizon engineering state

- facts/hypotheses/failure fingerprints;
- repository-entity-linked verified memory;
- bidirectional memory backlinks and temporal validity;
- trajectory compaction;
- negative memory;
- stagnation detector;
- fresh-context takeover;
- recovery ladder;
- invalidation triggered by spec/repo/dependency/build/environment/parser/semantic-adapter changes;
- optional read-only Markdown/Obsidian-compatible knowledge-notebook export for human inspection, never as canonical state.

## Exit gate

RepoIntelBench demonstrates that RI2 improves relevant source/symbol/line retrieval and/or engineering outcomes over the existing AER baseline and lexical-only/graph-only/embedding-only alternatives while staying within explicit latency/RAM/disk/token budgets.

The gate MUST additionally show:

- capability coverage reported by tier rather than a misleading raw language count;
- correct fallback when precise semantics are unavailable;
- no inferred edge presented as compiler-resolved truth;
- stale graph/memory relations invalidated under mutation tests;
- unchanged content reuses indexed artifacts;
- mixed-language/build-topology fixtures resolve representative imports/dependencies/tests;
- HandoffBench demonstrates lower rediscovery/token cost after forced context interruption with no material success degradation;
- stale environment/dependency/repository facts are invalidated correctly.

---

# Phase 7 — Parallelism, Resource Scheduling, and Integration

## Build

- dependency-aware scheduler;
- bounded ready/admission queues;
- weighted fairness and verifier capacity reservation;
- provider/resource reservations;
- write-set estimation;
- multiple isolated worktrees/sandboxes;
- branch-and-merge integration;
- semantic conflict checks;
- cancellation/preemption protocol;
- orphan-service cleanup.

## Exit gate

Selected parallelizable tasks improve wall-clock or success without unacceptable coordination cost. Adversarial scheduling/property tests show bounded resource usage, no starvation of verification, and no duplicated active leases.

---

# Phase 8 — Architecture Health Controller

## Build

- language-specific metric adapters that reuse the RI2 language capability registry;
- structural erosion/time-series tracking over the temporal repository graph;
- dependency-boundary rules;
- health delta gate;
- debt records/refactor triggers.

## Exit gate

EvolutionBench demonstrates reduced long-horizon deterioration versus the same runtime without health control. Architecture-health conclusions preserve the same provenance/capability distinctions as Repository Intelligence rather than treating heuristic graph structure as compiler truth.

---

# Phase 9 — Ecosystem Protocols, Skills, and Supply-Chain Hardening

## Build

- MCP current-spec adapter;
- Tasks extension where needed;
- optional A2A gateway;
- skill registry/router;
- dependency/package provenance policy;
- SBOM/provenance hooks;
- package-install/network policy;
- security/provenance labels.

## Exit gate

External protocol conformance/security tests pass without weakening internal authority. Dependency/supply-chain fixtures demonstrate pinning, provenance recording, and policy-controlled executable package behavior.

---

# Phase 10 — Compatibility, Migration, Distribution, and Release Hardening

## Build

- machine-readable compatibility matrix;
- database/event/IR/API migration framework;
- client/daemon feature negotiation;
- migration backup/checkpoint + crash injection;
- downgrade refusal/restore paths;
- release channels;
- signed/attested platform artifacts;
- secure update metadata and anti-rollback/freshness policy;
- release SBOM/provenance;
- installation/upgrade/rollback smoke suites.

## Exit gate

Representative old project states upgrade safely; crash at every migration boundary remains recoverable; incompatible client/state combinations fail closed with actionable diagnostics; release artifact/update verification passes on supported platforms.

---

# Phase 11 — Learned Policies and Self-Evolution Lab

## Build

- replay datasets;
- policy candidate framework;
- offline contextual router/retriever experiments;
- shadow/canary comparison;
- automatic regression reports;
- candidate prompt/retrieval/skill/verifier optimization.

## Exit gate

At least one learned or model-proposed policy passes held-out improvement thresholds and production safety gates. No policy self-promotes, and promotion uses the same signed/versioned release semantics as other trusted policy artifacts.

---

# Cross-cutting gates

Every phase MUST additionally preserve:

- bounded resource ownership (`39`);
- workspace/user-state safety (`41`);
- data governance and retention (`42`);
- executable contract/schema discipline (`44`);
- compatibility with already released durable contracts (`40`);
- security threat model (`19`);
- Repository Intelligence provenance/freshness/capability invariants (`06`) once RI2 contracts exist;
- architecture health when enough implementation exists to measure it.

A phase is not complete merely because its happy-path demo works.

# Release sequence

### `0.1` — reliable local single-agent runtime + executable contracts/resource safety  
### `0.2` — Engineering IR + research + context engine  
### `0.3` — reproducible proof verification + resilient provider/cost router  
### `0.4` — Repository Intelligence 2.0 + long-horizon recovery + bounded parallel worktrees  
### `0.5` — architecture health + protocol/supply-chain ecosystem  
### `0.8` — migration/release/update hardening on supported platforms  
### `1.0` — validated adaptive runtime with policy lab and stable compatibility promise

Version numbers are planning labels, not delivery promises.
