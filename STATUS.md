# everything Implementation Status

**Last updated:** 2026-08-17
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55` plus accepted architecture updates on `main`
**Public product / executable:** `everything`
**Internal architecture terminology:** AER remains valid where the architecture uses it
**Current phase:** Inter-step Provider Runtime Productization Gate
**Current step:** between 13 / 18 and 14 / 18
**Repository-side state:** MERGED + CI GREEN — real Claude transport reproduced on target Windows; provider gate remains OPEN on isolation/context-economy/telemetry acceptance
**Current main:** `1ba6206600d10a44aa6d5114a3510ad03806d205` before this documentation closeout; post-merge `foundation-ci` `31975867579` SUCCESS
**Provider implementation:** PR #6 merged; Windows fixture repair PR #7 merged; Claude smoke turn-limit repair PR #8 merged
**Provider gate:** delegated Codex/Claude/Gemini auth transports + Architecture Context Capsule + permission controller + AER ToolBroker + real-model smoke surface; live testing exposed provider-local hook contamination and excessive static context
**Next step:** Provider isolation + compact contextual bootstrap + complete usage telemetry; Step 14 remains BLOCKED until these acceptance gaps close

## Agent engineering policy

`AGENTS.md` is the canonical implementation temperament for coding agents. YAGNI, semantic DRY, dependency restraint, bounded resource use, fail-closed correctness, evidence-before-completion, and measured performance apply to all remaining implementation work. `CLAUDE.md` delegates to it rather than duplicating policy.

## User-directed product-surface freeze

The CLI/TUI remains intentionally frozen while the core architecture is completed. Until the user explicitly lifts this rule:

- do not add or redesign CLI/TUI features;
- do not expose new core capabilities through `crates/aer-cli`;
- do not use presentation work as a Step exit criterion;
- preserve the existing zero-redraw CLI only as a regression surface;
- develop and verify domain/core/storage/repository/context/runtime architecture first.

`crates/aer-cli/**` was not modified by Steps 10–13.

The user explicitly lifted the product-surface freeze for the inter-step provider/auth/tool/permission productization gate. This is a scoped exception: provider onboarding, model smoke I/O and `/permission` may evolve now; unrelated TUI redesign remains deferred.

## Completed milestones

- **Step 01 — Foundation Bootstrap:** COMPLETE — CI `31899011790`.
- **Step 02 — Executable Contract System:** COMPLETE — CI `31903313314`; target Windows PASS.
- **Phase 0:** COMPLETE.
- **Step 03 — Durable State Kernel:** COMPLETE — CI `31905250522`; target Windows PASS.
- **Step 04 — Runtime State + Resource Safety:** COMPLETE — CI `31906368065`; target Windows PASS.
- **Step 05 — Workspace + Execution Boundary:** COMPLETE — CI `31909059844`; target Windows PASS.
- **Step 06 — Single-Agent Runtime 0.1:** COMPLETE — CI `31911224304`; target Windows PASS.
- **Step 07 — Intent + Research + Engineering IR:** COMPLETE — semantic baseline `d5668b5d87a3b8a3f598b9cd016cc11cc5504837`; target Windows reproduction confirmed.
- **Step 08 — Repository Intelligence:** COMPLETE — code HEAD `12b97c6e9c715a19354af6ba5b661eb83ed9f353`; CI `31918025079`; target Windows canonical verification reproduced by the user on 2026-08-16.
- **Step 09 — Context Economy Engine:** COMPLETE — repository CI `31920562037`; target Windows canonical verification reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.
- **Step 10 — Verification + Proof System:** COMPLETE — repository CI `31939146224`; post-merge main CI `31939487328`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.
- **Step 11 — Provider Resilience + Cost Router:** COMPLETE — repository CI `31951923261`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.
- **Step 12 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery:** COMPLETE — repository CI `31957740270`; post-merge main CI `31958367494`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.
- **Step 13 — Bounded Parallel Execution:** COMPLETE — PR CI `31961188080`; post-merge main CI `31962148919`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.

The Step-10 target-machine reproduction also reconfirmed the checked-in documentation/contract inventory, immutable-verifier/proof tests, full workspace regression suite, and product build before Step 11 was started.

## Step 10 — Verification + Proof System

**State:** COMPLETE

### Ownership and scope

The first architecture-complete verification vertical slice lives in `aer-core::verification`.

Step 10 deliberately does **not** create an `aer-verify` crate merely because the target repository map names one. The current slice already depends on core orchestration, contracts, environment identity, execution, domain state transitions, and durable storage. A separate crate should be introduced only when independent ownership, dependency pressure, or testing boundaries make that split materially clearer.

The implementation is intentionally generic. Domain-specific checks are supplied through verification profiles; domain knowledge does not fork the task state machine or weaken organization-level gates.

### Independent verifier authority

`VerifierDefinition` describes a verifier by stable ID/version, verification layer, evidence type, executable/arguments, protected verifier assets, timeout/capture bounds, and isolation requirement.

`VerifierSnapshot` binds the definition digest to a deterministic recursive digest of protected verifier/test assets. Candidate verification re-hashes those assets before execution. A changed verifier definition, changed protected test, changed verifier asset, symlinked verifier asset, or path escape fails closed.

This is the Step-10 defense against a generator obtaining a false PASS by weakening the oracle it is being judged by.

### Verification composition and Domain Profiles

`VerificationPlan` starts from mandatory verifier/evidence requirements and composes every applicable `DomainProfile` by set union.

A lower/domain profile can add gates but cannot remove a mandatory verifier or evidence type. The bound plan also pins the exact verifier snapshots used for the run and derives a deterministic composition snapshot.

### Environment-bound evidence

Verifier execution reuses the existing `aer-exec` and `aer-environment` boundaries rather than introducing a parallel process runtime.

Every produced Evidence Record is bound to:

- exact repository snapshot;
- `EnvironmentFingerprint` digest;
- verifier ID/version;
- immutable verifier snapshot;
- command argv/cwd;
- input artifact hashes;
- stdout/stderr hashes and byte counts;
- exit/timing result;
- command-evidence digest;
- declared security profile.

Strong isolation is not simulated. When a verifier requires stronger isolation than the current direct executor can provide, execution fails closed before the verifier process is admitted.

### Evidence cache boundary

`EvidenceCacheKey` treats repository snapshot, environment fingerprint, verifier snapshot, and input artifact hashes as hard reuse boundaries.

A change in any of them makes prior evidence stale. Step 10 does not claim probabilistic or semantic cache equivalence.

### Proof-carrying acceptance

`build_proof_manifest` requires exact coverage of the task's requirement set. Each requirement must map to:

1. at least one current implementation location;
2. at least one passing Evidence Record that attests that requirement;
3. evidence from the same repository snapshot;
4. evidence carrying environment and verifier-integrity identity;
5. every verifier/evidence type required by the bound Verification Plan.

The generated Proof Manifest is validated through the executable schema registry and then through the existing cross-contract semantic validator. Generator-controlled verifier evidence cannot support an accepted proof.

### Durable acceptance chain

`persist_accepted_verification` validates the proof and the domain transition before persisting acceptance.

The authoritative sequence is:

1. store Evidence Records as content-addressed internal artifacts;
2. append `evidence.created` events;
3. store the passing Proof Manifest as a pinned artifact;
4. append `verification.verdict` referencing that proof;
5. append `task.accepted` causally linked to the verification verdict.

The existing `TaskState::Verifying -> TaskState::Accepted` guard remains authoritative and requires accepted proof. The verification slice does not bypass the state machine with a generic status write.

### Step-10 adversarial and invariant tests

The focused Step-10 test surface verifies that:

- deliberate protected-test/verifier tampering is detected;
- Domain Profiles can only strengthen mandatory verification;
- repository/environment/verifier/input changes invalidate evidence reuse;
- unsupported strong-isolation requirements fail closed;
- command evidence is bound to repo/environment/verifier identity;
- Proof Manifest construction requires an exact requirement -> implementation -> passing-evidence chain;
- stale-repository evidence cannot support a current proof;
- accepted verification persists evidence -> verdict/proof -> task acceptance in causal order and preserves journal integrity.

### Permanent verification gates

Step 10 added a permanent Linux CI gate:

```text
cargo +1.97.1 test --locked -p aer-core --all-targets verification
```

The canonical Windows verifier includes the corresponding target-specific Step-10 gate before the remaining storage/document/Phase-0/product checks.

The final repository-side verification run `31939146224` passed:

- workspace formatting;
- workspace-wide `-D warnings` Clippy;
- full workspace regression suite;
- Intent + Research + Engineering IR gate;
- Repository Intelligence gate;
- Context Economy gate;
- Verification + Proof integrity gate;
- Single-Agent Runtime gate;
- Workspace + execution boundary gate;
- CLI regression/zero-redraw guard;
- Durable State Kernel gate;
- documentation integrity;
- Phase-0 executable contract gate;
- canonical isolated Windows verification.

Temporary branch-only format/compile repair workflows used during implementation were removed after their exact repairs. No write-capable repair workflow is part of the verified Step-10 tree.

## Step 10 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Independent verifier definition + protected asset identity | PASS | `VerifierDefinition` + `VerifierSnapshot`. |
| Deliberate verifier/test tampering detection | PASS | `immutable_verifier_detects_deliberate_test_tampering`. |
| Safe relative verifier asset boundary | PASS | path validation + symlink/unsupported-asset rejection. |
| Mandatory verification cannot be weakened by Domain Profiles | PASS | monotone union composition + focused test. |
| Bound verifier composition snapshot | PASS | required snapshot resolution + deterministic composition digest. |
| Evidence bound to exact repository snapshot | PASS | `CommandExecutionEvidence` + Evidence Record construction. |
| Evidence bound to Environment Fingerprint | PASS | environment digest required and persisted. |
| Evidence bound to verifier identity/snapshot | PASS | verifier ID/version + integrity snapshot checks. |
| Evidence input/output artifact identity | PASS | SHA-256 input validation + captured output hashes. |
| Exact evidence cache invalidation boundary | PASS | repo/environment/verifier/input cache-key test. |
| Strong-isolation capability mismatch fails closed | PASS | direct executor refusal test. |
| Exact requirement -> implementation -> evidence coverage | PASS | proof builder coverage rules + focused proof test. |
| Stale repository evidence rejected | PASS | stale-evidence adversarial test. |
| Generator-controlled verifier evidence rejected | PASS | proof integrity guard + existing semantic validator. |
| Current Evidence Record schema validation | PASS | embedded executable contract registry. |
| Current Proof Manifest schema validation | PASS | embedded executable contract registry. |
| Cross-contract semantic proof validation | PASS | `validate_semantic_bundle`. |
| Accepted task requires passing proof | PASS | existing domain state-machine guard reused. |
| Durable evidence -> verdict/proof -> acceptance chain | PASS | persistence integration test + journal integrity verification. |
| No new third-party Step-10 dependency | PASS | implementation reuses existing workspace crates/dependencies. |
| No premature `aer-verify` crate split | PASS | YAGNI ownership decision documented above. |
| CLI/TUI freeze preserved | PASS | no `crates/aer-cli/**` changes. |
| Workspace-wide format | PASS | CI `31939146224`. |
| Workspace-wide `-D warnings` Clippy | PASS | CI `31939146224`. |
| Full workspace regression suite | PASS | CI `31939146224`. |
| Permanent Linux Verification + Proof CI gate | PASS | CI `31939146224`. |
| Canonical isolated Windows CI verifier including Step 10 | PASS | CI `31939146224`. |
| Temporary write workflow/repair scaffolding removed | PASS | verified Step-10 code HEAD `c48a9afa95e63467198a0ea251c100232f90b79b`. |
| Target Windows canonical verifier | PASS | user reproduction on 2026-08-16; final line `everything Windows verification: PASS`. |

Step 10 is closed. Its acceptance evidence remains in this ledger for replay/audit; Step 11 does not replace or weaken it.

## Step 11 — Provider Resilience + Cost Router

**State:** COMPLETE

### Ownership and scope

Step 11 evolves the existing `aer-provider` gateway in place. It does not create a parallel provider runtime and does not introduce live paid API requirements into deterministic correctness gates.

The implementation target is the architecture in:

- `docs/08_MODEL_CAPABILITY_REGISTRY.md`;
- `docs/09_ADAPTIVE_MODEL_ROUTER_AND_BUDGETS.md`;
- `docs/20_OBSERVABILITY_AND_COST_ACCOUNTING.md`;
- `docs/37_PROVIDER_GATEWAY_AND_RESILIENCE.md`.

### Normalized provider fault semantics

`ProviderFailureClass` now covers invalid request, authentication, authorization, content policy, rate limiting, quota exhaustion, transient unavailability, provider-internal failure, timeout, connection failure, stream interruption, schema violation, context overflow, cancellation, unknown failure, and the legacy Phase-1 compatibility classes.

Retry eligibility, circuit-breaking eligibility, and distinct-endpoint fallback eligibility are explicit and separate. Authentication/authorization failures are never blindly retried against the same endpoint.

### Capability and policy eligibility

`EndpointProfile` carries endpoint/provider/model/snapshot identity together with:

- context/output limits;
- structured output and tool-call capabilities;
- parallel tool-call, streaming, multimodal, prompt-cache, reasoning-control and cancellation capability flags;
- privacy sensitivity, retention and region constraints;
- credential usability;
- capability observation timestamp and TTL;
- endpoint-scoped health and quota state;
- pricing snapshot;
- tier, measured verified-success rate, p95 latency and architecture-risk signal.

Routing performs hard capability/security/privacy/region/snapshot/freshness/health/budget/latency/quality-floor filtering before utility optimization. A cheaper endpoint cannot override a hard policy requirement.

### Cost, health and rate-limit control

`PricingSnapshot` uses integer micro-USD-per-million-token rates. Cost estimation is overflow-checked and rounds fractional micro-USD charges upward rather than silently under-accounting them.

`EndpointHealth` maintains endpoint-scoped degraded/rate-limited/open-circuit/unavailable state. Transient failures increment bounded circuit state; success clears the transient failure streak.

`RateLimitWindow` provides local request/token reservations so concurrent local decisions cannot intentionally oversubscribe a known quota window.

### Deterministic routing and fallback

The first router policy exposes explicit user quality modes:

- `Economy` — minimize eligible estimated cost first;
- `Balanced` — deterministic quality/risk/latency/cost utility;
- `MaximumQuality` — maximize measured verified success first while respecting all hard constraints.

High-uncertainty work can route through an eligible scout tier without hard-coding provider/model names into policy. Stable endpoint identity is the final deterministic tie-break.

`ResilientProviderPool` composes the existing bounded `ProviderGateway` rather than replacing it. Gateway retries and cross-endpoint failovers have independent hard bounds. Fallback excludes the failed endpoint and re-runs eligibility over the remaining profiles.

### Inspectable decision evidence

Every logical provider call can retain an attempt trace containing:

- selected endpoint;
- direct/scout/fallback strategy;
- expected cost under the selected pricing snapshot;
- actual gateway attempt count;
- normalized terminal outcome.

The route decision separately retains eligible endpoint identities and hard rejection reasons for excluded candidates.

### ProviderBench and RouterBench

Step 11 adds deterministic scripted tests with no live accounts or paid requests.

ProviderBench verifies bounded retry followed by bounded distinct-endpoint failover while preserving the attempt trace.

RouterBench verifies that economy and maximum-quality modes choose different endpoints for the same eligible candidate set, that the cheaper route has lower estimated cost, and that policy behavior is independent of hard-coded model names.

### Permanent Step-11 gates

Linux CI now contains:

```text
cargo +1.97.1 test --locked -p aer-provider --test provider_router_bench
```

The canonical Windows verifier contains the equivalent target-specific ProviderBench/RouterBench command in addition to the full workspace suite.

The final repository-side Step-11 code tree at `163ba7903e719cefcb3595f025f12358c376babe` passed `foundation-ci` run `31951923261`:

- workspace formatting;
- workspace-wide `-D warnings` Clippy;
- full workspace regression suite;
- Intent + Research + Engineering IR gate;
- Repository Intelligence gate;
- Context Economy gate;
- Verification + Proof integrity gate;
- **Provider Resilience + Cost Router gate**;
- Single-Agent Runtime gate;
- Workspace + execution boundary gate;
- CLI regression/zero-redraw guard;
- Durable State Kernel gate;
- documentation integrity;
- Phase-0 executable contract gate;
- canonical isolated Windows verification including the focused Step-11 provider bench.

Temporary branch-only format/lint repair workflows were removed after their exact repairs. No write-capable repair workflow is part of verified Step-11 code HEAD `163ba7903e719cefcb3595f025f12358c376babe`.

## Step 11 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Expanded normalized failure taxonomy | PASS | `aer-provider::ProviderFailureClass` + focused unit tests. |
| Retry-safe vs non-retry-safe semantics | PASS | gateway retry predicate + authentication/transient tests. |
| Capability/privacy/region/snapshot eligibility | PASS | `routing::eligibility`. |
| Capability freshness/drift fails closed | PASS | capability TTL + stale-profile test. |
| Timestamped pricing + integer cost accounting | PASS | `PricingSnapshot::estimate_cost_micros` + round-up test. |
| Local rate-limit reservation | PASS | `RateLimitWindow::reserve` + oversubscription test. |
| Endpoint-scoped health + circuit breaker | PASS | `EndpointHealth` + open/cooldown/recovery test. |
| Deterministic economy/balanced/maximum-quality routing | PASS | `route` unit tests + RouterBench. |
| Scout routing under uncertainty | PASS | `ScoutThenRoute` decision test. |
| Bounded gateway retry | PASS | existing gateway + expanded failure semantics tests. |
| Bounded provider failover | PASS | `ResilientProviderPool` + ProviderBench. |
| Failed-attempt count reflects actual retry semantics | PASS | non-retryable authentication path records one attempt. |
| Inspectable routing/fallback attempt trace | PASS | `ProviderAttemptRecord` + ProviderBench assertions. |
| Adapter/profile identity binding | PASS | `validate_profile_binding`. |
| No live paid API dependency in correctness gates | PASS | scripted provider fixtures only. |
| CLI/TUI freeze preserved | PASS | no `crates/aer-cli/**` Step-11 changes. |
| Workspace format | PASS | CI `31951923261`. |
| Workspace `-D warnings` Clippy | PASS | CI `31951923261`. |
| Full workspace regression suite | PASS | CI `31951923261`. |
| ProviderBench + RouterBench Linux gate | PASS | CI `31951923261`. |
| Canonical isolated Windows CI verifier including focused Step 11 gate | PASS | CI `31951923261`. |
| Temporary repair scaffolding removed | PASS | verified Step-11 code HEAD `163ba7903e719cefcb3595f025f12358c376babe`. |
| Target Windows canonical verifier | PASS | user reproduction on 2026-08-16; final line `everything Windows verification: PASS`. |

## Step 11 exit condition

Step 11 is closed. The target Windows checkout reproduced the canonical verifier on 2026-08-16; its acceptance evidence remains above for audit/replay.

## Step 12 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery

**State:** COMPLETE

### Ownership and scope

Step 12 evolves the existing `aer-repo` index in place. It does not create a second repository index and does not claim that grammar coverage equals precise semantic coverage.

The implementation follows `docs/06_REPOSITORY_INTELLIGENCE.md`, `docs/07_CONTEXT_ECONOMY_ENGINE.md`, `docs/15_ENGINEERING_STATE_AND_MEMORY.md`, `docs/21_EVALUATION_AND_BENCHMARK_STRATEGY.md`, and the settled architecture decision matrix.

### Repository Intelligence 2.0

The v1 repository index is migrated to schema v2 while preserving SQLite WAL/FULL durability, exact workspace snapshot binding, content-addressed syntax reuse, and stale-snapshot refusal.

The RI2 uplift adds:

- a versioned language capability registry with explicit Tier-0 lexical fallback and pinned native Tree-sitter parser identities;
- a capability ladder that keeps syntax, project resolution, precise semantics and runtime evidence distinct;
- a provenance-bearing repository graph with bounded traversal/backlinks;
- evidence classes `extracted`, `semantic_resolved`, `observed`, and `inferred`, with confidence, producer/version, source anchors, repository snapshot and optional environment identity;
- stable repository entity IDs and exact symbol continuity where evidence supports it;
- dependency-aware invalidation and same-snapshot rebuild when parser/query/graph producer identity drifts;
- safe dirty-worktree deletion handling without weakening workspace TOCTOU checks;
- an optional precise-semantic ingestion boundary for compiler/LSP/SCIP-like producers that cannot silently impersonate syntax truth;
- bounded hybrid retrieval carrying why-relevant, capability, provenance, freshness and confidence.

### Tier-2 project topology

Cargo is the first concrete Tier-2 project resolver. AER invokes versioned machine-readable metadata with `--locked`, records package/target/dependency topology, and binds the project view to the discovered environment fingerprint.

Project metadata paths are normalized through the filesystem on both sides of the containment check so Windows verbatim/extended-length paths and ordinary drive paths cannot cause a false `Unavailable` project view. The same normalization remains fail-closed for paths that resolve outside the repository.

### Long-horizon engineering state and recovery

`aer-core` now keeps evidence-backed engineering memory separate from repository-derived truth. Verified facts, user decisions, assumptions, hypotheses, failure fingerprints and progress records retain explicit validity and invalidation scope.

Repository entity, spec, environment, dependency and producer changes can invalidate only the records bound to those dimensions. Disproven hypotheses and failure fingerprints form negative memory rather than being silently forgotten.

Handoffs compact verified state, unresolved dependencies and bounded relevant context instead of replaying the full transcript. Stagnation is assessed from repetition, new-evidence/entity yield and verifier progress, then escalated through a bounded typed recovery ladder ending in fresh-context takeover.

### Step-12 acceptance evidence

`repo_intel_2_bench` verifies capability-tier/fallback behavior, provenance correctness, exact-vs-inferred semantic separation, bounded graph traversal, incremental reuse/invalidation, producer freshness, mixed-language behavior and hybrid retrieval yield against lexical/current-AER/graph-only/deterministic embedding-only controls under explicit budgets.

`repo_intel_2_tier2` uses a real two-package Cargo workspace and verifies local dependency resolution, current Tier-2 capability, environment provenance and same-snapshot environment-drift rebuild.

`handoff_bench` verifies bounded compaction, targeted invalidation and calibrated/bounded stagnation recovery.

The canonical Windows verifier additionally exposed and closed three Windows-only correctness/portability issues before acceptance: durable-store test handles were explicitly closed before fixture deletion, HandoffBench store handles were explicitly closed before cleanup, and Cargo metadata paths are normalized symmetrically across Windows path forms.

### Permanent Step-12 gates

Linux CI contains explicit gates for:

```text
cargo +1.97.1 test --locked -p aer-repo --test repo_intel_2_bench
cargo +1.97.1 test --locked -p aer-repo --test repo_intel_2_tier2
cargo +1.97.1 test --locked -p aer-core --test handoff_bench
```

The canonical Windows verifier contains equivalent target-specific gates in addition to the full workspace regression suite.

The verified Step-12 code tree at `5446fb2d4887d0bc7b18174af6cc81bd349a837c` passed `foundation-ci` run `31957740270` on Ubuntu and the canonical isolated Windows verifier. All temporary Step-12 repair/verification workflows were removed from the final tree; `.github/workflows` contains only permanent `ci.yml`.

## Step 12 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Existing repository index evolved in place | PASS | `aer-repo` schema v2 migration; no second index. |
| Versioned language capability registry | PASS | RI2 language registry + capability report. |
| Universal safe-text fallback | PASS | Tier-0 fallback for non-native parser text fixtures. |
| Native Tree-sitter syntax identities are versioned | PASS | parser/cache identity includes adapter/runtime/query/registry versions. |
| Syntax does not impersonate precise semantics | PASS | separate Tier-1/Tier-3 views and evidence classes. |
| Provenance-bearing graph | PASS | graph nodes/edges + `EdgeEvidence`. |
| Inferred graph evidence remains distinguishable | PASS | `EvidenceClass::Inferred` + benchmark assertions. |
| Bounded graph traversal/backlinks | PASS | traversal depth/node/edge budgets + RepoIntelBench. |
| Cargo Tier-2 package/build topology | PASS | `cargo metadata --format-version 1 --no-deps --locked`. |
| Local project dependency resolution | PASS | two-package `repo_intel_2_tier2` fixture. |
| Tier-2 environment provenance | PASS | project view stores environment fingerprint. |
| Windows metadata path normalization | PASS | canonical metadata-path regression + Windows CI `31957740270`. |
| Precise semantic adapter boundary | PASS | snapshot/producer/environment-bound semantic ingestion. |
| Same-snapshot producer/parser drift rebuild | PASS | freshness benchmark. |
| Same-snapshot environment drift rebuild | PASS | Tier-2 benchmark. |
| Dirty tracked deletion handling | PASS | deletion/rename incremental benchmark without TOCTOU weakening. |
| Stable entity/symbol continuity | PASS | stable IDs + continuity records with evidence/confidence. |
| Dependency-aware invalidation frontier | PASS | repository change/invalidation benchmark. |
| Hybrid retrieval beats required controls in fixture | PASS | RepoIntelBench lexical/current-AER/graph/embedding controls under fixed result budget. |
| Persistent index resource bound | PASS | benchmark-enforced fixture storage ceiling. |
| Evidence-backed long-horizon memory | PASS | typed engineering records + durable replay. |
| Repository/spec/environment/dependency/producer invalidation | PASS | engineering invalidation tests. |
| Negative memory | PASS | disproven hypothesis/failure fingerprint retention. |
| Bounded handoff compaction | PASS | HandoffBench. |
| Stagnation detection + bounded recovery ladder | PASS | HandoffBench calibrated progress window. |
| Windows durable-store handle lifetime | PASS | Windows CI regression. |
| Windows HandoffBench handle lifetime | PASS | Windows CI regression. |
| Workspace-wide format | PASS | CI `31957740270`. |
| Workspace-wide `-D warnings` Clippy | PASS | CI `31957740270`. |
| Full workspace regression suite | PASS | CI `31957740270`. |
| Permanent RepoIntelBench Linux gates | PASS | CI `31957740270`. |
| Permanent HandoffBench Linux gate | PASS | CI `31957740270`. |
| Canonical isolated Windows verifier including Step 12 | PASS | CI `31957740270`. |
| Temporary Step-12 workflow scaffolding removed | PASS | final workflow tree contains only `ci.yml`. |
| CLI/TUI freeze preserved | PASS | no `crates/aer-cli/**` Step-12 changes. |
| Target Windows canonical verifier | PASS | user reproduction on 2026-08-16; final line `everything Windows verification: PASS`. |

## Step 12 exit condition

Step 12 is closed. The target Windows checkout reproduced the canonical verifier on 2026-08-16; its acceptance evidence remains above for audit/replay.

## Step 13 — Bounded Parallel Execution

**State:** COMPLETE

### Ownership and scope

Step 13 adds bounded parallel execution without replacing the authorities established in earlier phases:

- `aer-domain::scheduling` owns deterministic dependency/fairness/conflict policy;
- the existing `RuntimeSafetyKernel`, `ResourceGovernor`, leases and cancellation protocol remain authoritative for resource and attempt ownership;
- `aer-workspace::parallel` owns branch-backed isolated worktrees and integration Git operations;
- `aer-core::parallel` coordinates those authorities and owns the integration barrier, measured utility policy and bounded orphan registry.

There are no fixed Planner/Coder/Reviewer/Tester personas. Parallelism remains a conditional optimization over independently verifiable work intents, and the CLI/TUI remains frozen.

### Dependency-aware bounded scheduler

`TaskGraph` validates duplicate/self/unknown dependencies and rejects cycles before scheduling. Dependency-satisfied pending tasks can become ready through the existing `TaskState` transition rules.

`BoundedScheduler` applies a hard ready-set bound and hard active-task bound. Candidate order is deterministic and combines weighted per-run service accounting with the documented priority signals: unblock value, critical path, risk reduction, information gain, user waiting, age, expected cost and merge-conflict risk.

Run weights provide deterministic weighted fairness; service accounting prevents one run from consuming every parallel slot merely because it has more ready tasks. Parallelism is denied or serialized when contracts are unstable, local verification is unavailable, resource demand is unknown, a task is explicitly serial-only, high-risk serialization applies, or predicted write/semantic ownership overlaps active work.

### Predicted and observed conflict control

`WriteScope` stores normalized repository-relative predicted/observed write prefixes and rejects host-absolute/path-escape scopes. Prefix overlap is component-aware, so `src/auth` overlaps `src/auth/token.rs` but not `src/authz`.

`SemanticWriteSet` separately represents shared API/schema/requirement-like ownership keys. The scheduler blocks predicted textual and semantic overlap before admission, while runtime observations can detect unpredicted actual write expansion before integration.

### Resource, lease, cancellation and preemption coordination

`ParallelRuntimeCoordinator` does not duplicate resource or lease truth. Scheduler bookkeeping is coupled to authoritative `RuntimeSafetyKernel::start_task`; a denied resource/lease admission rolls scheduler ownership back rather than leaving a ghost active task.

Existing verifier reservation therefore remains effective under generator load. Verification/cancellation finalization releases authoritative runtime ownership before scheduler ownership is removed.

Preemption is conservative: only lower-priority generator work explicitly marked discardable/checkpointable can be selected. External-mutating attempts and `PreemptionSafety::Never` work are excluded.

High-risk and explicitly serial-only work create a bidirectional serialization barrier: they cannot join already-active work, and once active they block later parallel admission until their ownership is released.

### Isolated branch-backed worktrees

A captured exact `WorkspaceSnapshot` is first materialized into an AER-owned integration worktree. Dirty tracked/untracked user state is captured into an internal deterministic baseline commit on the integration branch; the user's branch and working tree are never changed.

Every writable parallel task receives a dedicated branch/worktree forked from the same integration baseline for its wave. Task change sets record base/head identity, actual changed paths and dirty state.

Integration merge commits use AER-owned deterministic Git author/committer identity rather than depending on the user's global Git identity. Worktree discovery is bounded through Git's porcelain worktree inventory and branch-prefix ownership.

### Integration barrier

Branch-local verification is necessary but insufficient. `LocalBranchEvidence` requires a clean committed change plus local evidence. `IntegrationPlan` requires one common base, bounded candidate count and no duplicate task evidence; exact changed-path overlap and semantic ownership overlap fail before merge.

Branches are merged into the isolated integration worktree in deterministic task order. `IntegrationBarrier` refuses acceptance until all planned merges are recorded and an integration-aware verification result is bound to the exact final integration head, repository snapshot, environment fingerprint and proof-manifest identities.

Immediately before each merge, the integration worktree re-resolves the task branch ref and requires it to equal the locally verified `head_commit`. It also re-validates base→head ancestry and recomputes the exact changed-path set from Git. A branch commit added after local verification, a fabricated base, or stale/incomplete changed-path evidence therefore fails closed before integration.

### Bounded orphan cleanup

`OrphanRegistry` records worktrees, process trees, ephemeral services, local ports and containers with task ownership and cleanup deadlines. Registry size and cleanup-per-sweep are hard-bounded. Live owners are never swept; failed cleanup remains registered for recovery; duplicate registration fails before mutation so ownership authority cannot be overwritten by an error path.

### Measured parallel utility

`ParallelUtilityMeasurement` compares serial wall time, parallel wall time plus coordination overhead, verified-success count and monetary cost under an explicit `ParallelUtilityPolicy`. Parallelism is not assumed to be useful merely because two slots exist.

ResourceBench includes a real wall-clock control using two independent bounded sleep workloads: it measures the same work serially and concurrently and requires the parallel measurement to satisfy the utility policy. This is deliberately deterministic/tool-only and has no paid provider dependency.

### ResourceBench

The Step-13 ResourceBench verifies:

- generator saturation cannot consume the reserved verifier slot;
- denied authoritative resource admission rolls back scheduler ownership;
- ready-set and active-worker counts remain bounded;
- weighted fairness surfaces work from competing runs;
- predicted-disjoint tasks that expand into the same actual write scope are detected;
- external mutation/non-preemptible work cannot be chosen for preemption;
- orphan cleanup is bounded, live-owner-safe and retry-preserving;
- duplicate orphan registration cannot overwrite authority;
- disjoint files with conflicting semantic ownership are rejected;
- branch-local PASS cannot bypass integration verification/proof;
- post-verification branch mutation is rejected because merge-time branch head/base/changed-path evidence is re-measured;
- high-risk serialization is enforced in both admission directions;
- real dirty-user-state snapshots can fork two isolated task worktrees, merge verified branches, and leave the user's branch/tree unchanged;
- measured parallel wall-clock utility is positive against its serial control.

### Permanent Step-13 gates

Linux CI contains:

```text
cargo +1.97.1 test --locked -p aer-core --test resource_bench
```

The canonical Windows verifier runs the equivalent target-specific ResourceBench command in addition to the full workspace suite.

## Step 13 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Dependency graph validates unknown/self/cyclic dependencies | PASS | `aer-domain::scheduling::TaskGraph` tests. |
| Hard ready-set and active-worker bounds | PASS | `SchedulerPolicy` + ResourceBench. |
| Deterministic weighted run fairness | PASS | service/weight ordering + ResourceBench competing-run fixture. |
| Unknown resource demand fails closed | PASS | scheduler block reason + existing Resource Governor invariant. |
| Verifier reservation survives generator saturation | PASS | authoritative Resource Governor + ResourceBench. |
| Denied runtime admission rolls scheduler ownership back | PASS | `ParallelRuntimeCoordinator::admit_task` + ResourceBench. |
| Predicted repository write overlap blocks parallelism | PASS | component-aware `WriteScope`. |
| Semantic ownership overlap blocks parallelism | PASS | `SemanticWriteSet` + scheduler/integration tests. |
| Actual runtime write expansion is detected | PASS | observed write-scope ResourceBench fixture. |
| Exact dirty user state becomes isolated internal baseline | PASS | real Git worktree ResourceBench fixture. |
| One branch/worktree per writable parallel task | PASS | `aer-workspace::parallel`. |
| User active branch/tree not mutated by parallel workers | PASS | real worktree ResourceBench assertions. |
| Branch-local verification is not final acceptance | PASS | `LocalBranchEvidence` + `IntegrationBarrier`. |
| Exact final integration head is proof-bound | PASS | `IntegrationVerification`/`IntegratedAcceptance`. |
| Safe preemption excludes external mutation/non-preemptible attempts | PASS | scheduler candidate rules + ResourceBench. |
| Cancellation uses existing bounded protocol | PASS | `RuntimeSafetyKernel` coordination. |
| Orphan registry and cleanup are bounded | PASS | hard record/sweep limits + ResourceBench. |
| Failed orphan cleanup remains recoverable | PASS | ResourceBench retry fixture. |
| Duplicate orphan registration is mutation-free on failure | PASS | fail-before-insert rule + ResourceBench. |
| Parallel utility is measured, not assumed | PASS | serial/parallel wall-clock ResourceBench control. |
| No live/paid provider dependency in correctness gates | PASS | deterministic local fixtures only. |
| CLI/TUI freeze preserved | PASS | no `crates/aer-cli/**` Step-13 changes. |
| Focused Ubuntu format + `-D warnings` + domain/workspace/core regression | PASS | temporary read-only implementation gate before permanent CI. |
| Permanent Linux ResourceBench gate | PASS | CI `31961188080`. |
| Canonical isolated Windows verifier including ResourceBench | PASS | CI `31961188080`. |
| Full workspace regression suite | PASS | CI `31961188080`. |
| Temporary Step-13 workflow scaffolding removed | PASS | final workflow tree contains only permanent `ci.yml`. |
| Target Windows canonical verifier | PASS | user reproduction on 2026-08-16; final line `everything Windows verification: PASS`. |

## Step 13 exit condition

Step 13 is closed. The final production tree passed authoritative Linux + canonical Windows CI, was merged to `main`, post-merge CI passed, and the target Windows checkout reproduced `scripts/verify-windows.ps1` successfully on 2026-08-16.

## Inter-step Provider Runtime Productization Gate

**State:** IMPLEMENTED + MERGED + CI GREEN — TARGET CLAUDE TRANSPORT LIVE, PRODUCT ACCEPTANCE STILL OPEN

This is a non-numbered gate between Step 13 and Step 14. It productizes capabilities deliberately left reference-only while the safety, proof and scheduling backbone was being established. It does not create a nineteenth roadmap step.

### Authentication and provider transport

- Codex, Claude Code and Gemini CLI are represented by typed delegated provider adapters.
- Subscription login remains vendor-owned: AER launches documented provider authentication UX but does not scrape browser cookies, copy refresh tokens, or parse undocumented credential databases.
- Codex supports browser ChatGPT login and official device-code login through the delegated adapter.
- Claude delegates to `claude auth login/status/logout`.
- Gemini delegates Google sign-in to Gemini CLI's interactive authentication UX; because Gemini does not expose a stable standalone non-interactive Google-login status command, the smallest read-only model call is the authoritative connectivity check.
- Live smoke calls use bounded, machine-readable headless transports and run in an AER temporary workspace with provider-native mutation disabled/plan-only.
- Secret API-key environment variables are not inherited by delegated subscription smoke subprocesses by default.

### Shared model identity

`ArchitectureContextCapsule` gives every provider the same bounded, provider-neutral architecture identity before task-specific context. The capsule is source-hashed and currently binds `AGENTS.md`, `STATUS.md`, `docs/00_READ_ME_FIRST.md`, `DEVELOPMENT_PLAN.md`, and the provider/tool runtime specification when present.

Provider-native compatibility files (`CLAUDE.md`, `GEMINI.md`, and provider-native support for `AGENTS.md`) remain convenience bootstraps; they are not the authority boundary.

### Permission controller

AER now has a typed session permission controller with four user-facing modes:

- `plan`: reads only; non-read actions denied;
- `default`: reads automatic; every other eligible side effect asks;
- `auto`: reads, isolated-worktree writes and local process execution automatic; higher-impact effects ask;
- `full`: all actions already inside the runtime capability ceiling automatic.

`full` cannot grant privileged host authority. Explicit session deny overrides every mode. `/permission` exposes mode and session override control without allowing model text/provider output to widen the ceiling.

### AER ToolBroker vertical slice

The first native tool hot path is real, typed and bounded:

- `fs.read` — canonical workspace-contained bounded line/range reads with content hash;
- `fs.list` — deterministic bounded directory inventory;
- `exec.run` — structured argv/cwd execution through the existing execution policy with timeout, bounded previews and stdout/stderr hashes;
- `tool.search` — small metadata search without injecting all schemas;
- `tool.describe` — full schema only for the selected tool.

Provider-native tools are deliberately disabled during the initial real-model smoke. The next protocol-level agent loop must bridge structured provider tool proposals to this AER ToolBroker; it must not parse decorative terminal output or treat provider YOLO/bypass modes as AER authority.

### Product surface

The provider surface remains lazy and does not spawn/discover providers during ordinary CLI startup:

```text
everything providers
everything provider status [codex|claude|gemini]
everything provider login <provider>
everything provider login codex --device
everything provider logout <provider>
everything provider smoke <provider> --show-input --prompt "..."
everything provider smoke <provider> --json --prompt "..."
```

The interactive shell exposes `/providers`, `/provider ...`, and `/permission ...` equivalents.

### Acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Step 13 target Windows reproduction | PASS | user-provided canonical verifier log, 2026-08-16. |
| Delegated provider descriptors and aliases | PASS | `aer-provider::delegated` tests. |
| Vendor-owned auth boundary; no OAuth token scraping/storage | PASS | delegated adapter + normative docs. |
| Bounded Architecture Context Capsule | PASS | deterministic source/digest/budget test. |
| Default read-auto / non-read-ASK policy | PASS | permission-controller unit test. |
| Full autonomy cannot create Privileged authority | PASS | capability-ceiling unit test. |
| Explicit session deny overrides full | PASS | permission-controller unit test. |
| Bounded exact-limit provider capture | PASS | exact-limit/overflow adversarial test. |
| Codex JSONL final-output/usage parser | PASS | delegated parser test. |
| Claude/Gemini JSON final-output parser | PASS | delegated parser test. |
| Progressive Tool ABI disclosure | PASS | `tool.search`/`tool.describe` tests. |
| Structured `exec.run` command evidence | PASS | Auto-mode real `git --version` ToolBroker test. |
| Process execution requires AER-owned worktree authority | PASS | production constructor requires `OwnedWorktree`; read-only broker fails closed even in Auto mode. |
| Default mode command approval request | PASS | ToolBroker permission test. |
| Plan mode command denial | PASS | ToolBroker permission test. |
| Lazy provider CLI routing | PASS | provider CLI unit test. |
| Full workspace `-D warnings` Clippy | PASS | implementation CI before final permanent gates. |
| Full workspace unit/regression tests | PASS except docs inventory before manifest refresh | runtime tests passed; manifest subsequently regenerated. |
| Docs manifest covers provider runtime specification | PASS | regenerated `docs/MANIFEST.sha256`. |
| Permanent Linux provider/permission/tool gate | PASS | PR #6 `foundation-ci` `31971591717`; post-merge main CI `31972088680`. |
| Canonical Windows provider/permission/tool gate | PASS | PR #6 Windows CI; post-merge main CI `31972088680`. |
| Windows ToolBroker fixture cleanup robustness | PASS | target reproduction exposed teardown locking; PR #7 fixed collision-safe fixture identity + bounded transient-lock cleanup; PR/main Windows CI passed. |
| Claude delegated smoke `--max-turns 1` incompatibility | PASS | target live call exposed `terminal_reason=max_turns`; PR #8 removed the redundant cap; final `main` `1ba6206…` CI `31975867579` passed. |
| Temporary provider repair/finalization workflows removed | PASS | permanent workflow tree remains `.github/workflows/ci.yml` only. |
| Target Windows provider discovery | PARTIAL | Claude `2.1.233` authenticated on Claude.ai Pro; Codex PATH resolves to an invalid Win32 shim (`os error 193`); Gemini CLI unavailable. |
| Target-machine real Claude model transport | PASS | authenticated Claude print-mode call returned machine JSON and normalized final output; latest trace duration `43321 ms`, output tokens reported `2576`, raw event count `1`. |
| Architecture context reaches real model | PARTIAL | capsule digest/source list was transmitted and the response referenced AER/`AGENTS.md` constraints, but provider-local DeepWork behavior contaminated the answer. |
| Provider-local behavioral isolation | FAIL / OPEN | global Claude Code hook/skill behavior reached the delegated subprocess and displaced the requested AER Q&A response; vendor auth may be inherited, vendor-local behavioral policy may not silently become AER authority. |
| Context economy for provider bootstrap | FAIL / OPEN | prior raw Claude invocation reported `32563` cache-creation input tokens for the static architecture payload; production bootstrap must become compact invariant core + task-relevant RI2/context retrieval. |
| Complete provider usage telemetry | FAIL / OPEN | normalized trace currently under-reports effective input because cache-creation/cache-read/thinking/cost dimensions are not preserved separately. |
| Target-machine delegated OAuth + real model smoke acceptance | PARTIAL | OAuth + inference + parsing work for Claude, but relevance/isolation/context-economy/telemetry acceptance is not yet satisfied. |

## Provider productization exit condition

Do **not** start Step 14 until the clean production tree passes authoritative Linux + canonical Windows CI, is merged to `main`, the target Windows checkout remains reproducible, and at least one delegated provider completes a real authenticated model call that is **AER-controlled rather than provider-local-policy controlled**. Closure now additionally requires: (1) provider-local hooks/skills/config cannot silently redirect AER behavior; (2) the stable architecture bootstrap is compact and task-specific context is retrieved through RI2/Context Economy rather than repeatedly shipping a ~30k-token static payload; and (3) usage receipts preserve effective input, cache creation/read, output, thinking when reported, cost/model identity and latency. Provider-specific local unavailability is recorded explicitly rather than fabricated.
