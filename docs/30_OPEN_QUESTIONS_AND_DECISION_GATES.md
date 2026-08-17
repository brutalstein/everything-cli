# Open Questions and Decision Gates

This blueprint freezes principles while leaving implementation choices open where evidence is still required.

A coding agent MUST NOT arbitrarily close these questions without the required benchmark, acceptance evidence and, where architecture semantics change, an ADR.

## Q1 — Local RPC transport

**Baseline candidate:** Protocol Buffers + gRPC over loopback.  
**Alternatives:** JSON-RPC/HTTP, Connect, custom framed protocol.  
**Decision gate:** prototype CLI↔daemon streaming, cancellation, Windows/Linux support and binary/API evolution.

## Q2 — Default vector index

**Requirement:** persistent, local, cross-platform, replaceable.  
**Decision gate:** benchmark build/update/search latency and operational complexity on representative repositories.

Vector search is optional and never replaces exact lexical/structural lookup for exact identifiers.

## Q3 — Lexical engine

Candidates: SQLite FTS5, Tantivy, direct ripgrep + ranking.  
Select using repository-scale and incremental-update benchmarks.

## Q4 — Strong local sandbox backends by OS

The contract is frozen; implementation backend is not.

Decision gates:

- Linux filesystem/network isolation strength and startup cost;
- Windows native vs WSL2/microVM usability;
- macOS isolation practicality;
- package-install/network broker integration.

This question remains directly relevant to provider-native agentic tool execution: `DirectHostProcess` is not a strong sandbox.

## Q5 — Router learning algorithm

Do not choose before execution history exists.

Candidate family:

- contextual bandit;
- value model;
- Thompson/UCB-style safe exploration.

Decision metric: verified quality/cost regret with safety constraints.

## Q6 — Context fusion weights

Start with interpretable RRF + heuristics. Learn only after ContextBench data exists.

## Q7 — Architecture-health tooling per language

Need adapters and calibrated deltas. Avoid one universal metric.

## Q8 — Remote execution architecture

Not a v0.x blocker. Decide only after local scheduling/sandbox contracts are stable.

## Q9 — A2A exposure model

Whether AER acts primarily as A2A client, server, or both depends on enterprise integration demand. Keep the gateway isolated from the internal ABI.

## Q10 — Self-evolution autonomy

Baseline is offline proposal/eval/promotion. Any future autonomous canary promotion requires a separate security/evaluation ADR.

## Q11 — Secure update implementation

**Frozen requirement:** signed/attested release artifacts, freshness/anti-rollback protection, version/channel policy and recoverable state migration.

**Open implementation:** choose TUF library/service integration or an equivalent mature secure-update design based on Rust support, cross-platform packaging, offline install support and operational burden.

Do not replace the requirement with a custom unsigned `latest.json`.

## Q12 — Supply-chain metadata implementation

**Frozen requirement:** dependency/build/release provenance must be recordable and policy-verifiable.

**Open implementation:** exact SBOM/provenance/signing toolchain is selected through CI/tooling compatibility and enterprise demand.

## Q13 — Provider health algorithm constants

The normalized error/circuit/rate-limit semantics in `37` are frozen. Exact retry backoff, breaker thresholds and adaptive quota estimator are benchmarked/fault-injected per provider.

Do not hard-code one provider's headers/limits into core policy.

## Q14 — Domain profile catalog

The composition mechanism in `43` is frozen. The exact first-party domain profiles and language/tool adapters should be earned by representative user workloads and verification coverage.

## Q15 — Packaging format by platform

Choose supported install/update packaging after measuring:

- Windows signing/install/update UX;
- macOS notarization/package behavior;
- Linux distro-independent binary/package expectations;
- shell completion/manpage integration;
- rollback and migration behavior.

The internal compatibility contract is independent of packaging choice.

## Q16 — Claude authority-split production promotion

**Frozen requirement:** provider authentication, AER authority and task/repository evidence remain distinct. Repository text and provider-local behavior cannot gain control-plane authority.

**Current candidate:** for delegated Claude, replace the generic Claude Code default system preset with:

```text
SYSTEM:
  AER stable constitutional authority
  + delegated transport/security policy

USER/DATA:
  task-specific RI2 / Context Economy evidence
  + user objective
```

Current target-Windows diagnostics show materially lower provider input/cost than the current preset and the adversarial authority test passes.

**Decision gate:** do not promote the candidate until `docs/47_PROVIDER_AUTHORITY_SPLIT_ACCEPTANCE.md` is current and every required candidate measurement is valid, every acceptance contract passes, the adversarial case passes and no current-PASS task becomes candidate-FAIL.

There is deliberately no hard-coded economic savings threshold. Economics is evidence after correctness/authority eligibility.

## Q17 — Exact-identifier defining-span closure

The latest provider acceptance matrix exposed a repository/context correctness gap: the task named `ArchitectureContextCapsule::compile` and asked for its `version`, but the selected `model_context.rs` span stopped before the actual `version: 3` assignment. Both provider profiles then answered `1`.

**Frozen requirement:** when the task explicitly identifies a symbol/identifier and requests a concrete source-defined fact, the Context Pack must include the exact defining source span or fail closed/abstain.

**Implementation decision still open:** determine the smallest in-place RI2 + Context Economy correction. Candidate mechanisms include:

- exact identifier hit promoted to mandatory source coverage;
- syntax/symbol definition expansion to the enclosing initializer/return construction;
- expansion-handle escalation when the first structural span does not contain the requested fact;
- explicit required-semantic-coverage failure when the defining span cannot be proven.

Constraints:

- do not build a second retriever;
- do not globally inflate every context budget;
- do not treat a nearby structural span as sufficient simply because the path/symbol matched;
- preserve snapshot/provenance/freshness boundaries.

**Decision gate:** deterministic regression must retrieve the source anchor containing `version: 3`, remain within the bounded Context Pack policy and abstain if exact coverage cannot be established.

## Current sequencing effect

The present blocking sequence is:

```text
Q17 exact-definition retrieval repair
        ↓
full deterministic Linux + Windows verification
        ↓
rerun complete live Claude authority-split matrix
        ↓
Q16 production promotion decision
        ↓
Provider Runtime Productization Gate closure
        ↓
Step 14
```

Do not start ContextSizer tuning or Step 14 as a substitute for closing the retrieval correctness gap.
