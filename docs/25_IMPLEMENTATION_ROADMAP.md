# Implementation Roadmap

## Rule for coding agents

Do not jump to “full multi-agent self-improving platform.” Implement foundations in the order below. Each phase has an exit gate.

The reason for this order is causal: routing requires outcome data; self-evolution requires evals; multi-agent requires reliable single-agent state/integration; long-horizon autonomy requires sandboxing and resumability.

---

# Phase 0 — Repository Constitution and Eval Skeleton

## Build

- repository structure,
- Rust workspace and CI,
- schema package,
- deterministic test harness,
- docs/ADR enforcement,
- benchmark fixture interface,
- OpenTelemetry plumbing skeleton.

## Exit gate

- cross-platform build/test on Linux + Windows; macOS if available,
- schema validation tests,
- event/state machine property tests scaffold,
- no model provider dependency in core domain package.

---

# Phase 1 — Durable Single-Agent Runtime

## Build

- `aer-core`, `aerd`, `aer` CLI,
- SQLite event journal,
- project/run/task state machines,
- model provider abstraction with one reference adapter,
- tool ABI,
- basic isolated worktree lifecycle,
- command execution backend,
- resumable run/cancellation.

## Deliberately exclude

- learned router,
- parallel agents,
- vector retrieval,
- self-evolution.

## Exit gate

A simple coding task can be started, interrupted, resumed, executed in an isolated workspace, verified, and accepted with event replay.

---

# Phase 2 — Intent Engine and Engineering IR

## Build

- interactive requirement session,
- uncertainty representation,
- selective question policy,
- Engineering IR compiler,
- JSON Schema validation,
- SpecDelta/versioning,
- semantic-checksum workflow.

## Exit gate

Greenfield prompts produce stable versioned specs; omitted/distorted critical constraints are caught by the semantic-checksum eval suite.

---

# Phase 3 — Repository Intelligence and Context Economy

## Build

- commit-aware file inventory,
- lexical search,
- Tree-sitter parsing,
- symbol graph,
- git/test/runtime links,
- optional embeddings,
- retrieval fusion,
- Context Pack creation,
- token budgeter,
- extractive compression.

## Exit gate

ContextBench demonstrates better relevant-context yield/token than naive whole-map and single-retriever baselines.

---

# Phase 4 — Verification and Proof System

## Build

- evidence schema/collector,
- verifier composition,
- immutable verification workspace,
- requirement-to-evidence mappings,
- Proof Manifest,
- reward-hacking checks,
- independent semantic verifier adapter,
- architecture-health baseline hooks.

## Exit gate

Accepted tasks can be audited from requirement to code to immutable evidence. Integrity tests catch deliberate test weakening and verifier tampering.

---

# Phase 5 — Observability, Cost, and Deterministic Routing

## Build

- model capability registry,
- token/cache/cost accounting,
- task feature extraction,
- deterministic cost/quality router,
- scout-then-escalate policy,
- effort/context budget selection.

## Exit gate

RouterBench shows equal-or-better verified success at meaningfully lower normalized cost than “always use strongest model” on the project suite, without critical regressions.

---

# Phase 6 — Recovery and Long-Horizon State

## Build

- hypothesis/failure records,
- trajectory compaction,
- negative memory,
- stagnation detector,
- fresh-context takeover,
- recovery ladder.

## Exit gate

HandoffBench demonstrates lower rediscovery/token cost after forced context interruption with no material success degradation.

---

# Phase 7 — Parallelism and Integration

## Build

- dependency-aware scheduler,
- write-set estimation,
- multiple isolated worktrees/sandboxes,
- branch-and-merge integration,
- semantic conflict checks,
- parallel budget policy.

## Exit gate

Selected parallelizable benchmark tasks improve wall-clock or success without unacceptable coordination cost; non-parallel tasks remain single-worker.

---

# Phase 8 — Architecture Health Controller

## Build

- language-specific metric adapters,
- structural erosion/time-series tracking,
- dependency-boundary rules,
- health delta gate,
- debt records/refactor triggers.

## Exit gate

EvolutionBench demonstrates reduced long-horizon deterioration versus the same agent runtime without health control.

---

# Phase 9 — Ecosystem Protocols and Skills

## Build

- MCP 2026-07-28 adapter,
- Tasks extension support where needed,
- A2A v1.0 gateway,
- skill registry/router,
- security/provenance labels.

## Exit gate

External protocol conformance/security tests pass without weakening AER's internal authority model.

---

# Phase 10 — Learned Policies and Self-Evolution Lab

## Build

- replay datasets,
- policy candidate framework,
- offline contextual router,
- shadow/canary comparison,
- automatic regression reports,
- candidate prompt/retrieval/skill optimization.

## Exit gate

At least one learned or model-proposed policy passes held-out improvement thresholds and production safety gates. No policy self-promotes.

---

# Release sequence

### `0.1` — reliable local single-agent runtime
### `0.2` — Engineering IR + context engine
### `0.3` — proof-carrying verification + cost router
### `0.4` — long-horizon recovery + parallel worktrees
### `0.5` — architecture health + protocol ecosystem
### `1.0` — validated adaptive runtime with policy lab

Version numbers are planning labels, not delivery promises.
