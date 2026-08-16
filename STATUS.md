# everything Implementation Status

**Last updated:** 2026-08-16  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55` plus accepted architecture updates on `main`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 5 — Provider Resilience + Cost Routing  
**Current step:** 11 / 18 — Provider Resilience + Cost Router  
**Repository-side state:** IMPLEMENTATION IN PROGRESS — branch `agent/step-11-provider-resilience-cost-router`  
**Next step:** 12 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery — BLOCKED until Step-11 repository CI and target Windows verification pass

## Agent engineering policy

`AGENTS.md` is the canonical implementation temperament for coding agents. YAGNI, semantic DRY, dependency restraint, bounded resource use, fail-closed correctness, evidence-before-completion, and measured performance apply to all remaining implementation work. `CLAUDE.md` delegates to it rather than duplicating policy.

## User-directed product-surface freeze

The CLI/TUI remains intentionally frozen while the core architecture is completed. Until the user explicitly lifts this rule:

- do not add or redesign CLI/TUI features;
- do not expose new core capabilities through `crates/aer-cli`;
- do not use presentation work as a Step exit criterion;
- preserve the existing zero-redraw CLI only as a regression surface;
- develop and verify domain/core/storage/repository/context/runtime architecture first.

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
- **Step 09 — Context Economy Engine:** COMPLETE — repository CI `31920562037`; target Windows canonical verification reproduced by the user on 2026-08-16.
- **Step 10 — Verification + Proof System:** COMPLETE — repository CI `31939146224`; post-merge main CI `31939487328`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.

The Step-10 target reproduction reconfirmed the immutable-verifier/proof tests, full workspace tests, documentation integrity, Phase-0 executable contracts, and final product build before Step 11 started.

## Step 11 — Provider Resilience + Cost Router

**State:** IMPLEMENTATION IN PROGRESS

### Scope

Step 11 evolves the existing `aer-provider` gateway in place. It does not create a parallel provider runtime and does not introduce live paid API requirements into deterministic correctness gates.

The implementation target is the architecture in:

- `docs/08_MODEL_CAPABILITY_REGISTRY.md`;
- `docs/09_ADAPTIVE_MODEL_ROUTER_AND_BUDGETS.md`;
- `docs/20_OBSERVABILITY_AND_COST_ACCOUNTING.md`;
- `docs/37_PROVIDER_GATEWAY_AND_RESILIENCE.md`.

### Implemented in the current branch

- expanded provider-neutral failure taxonomy covering invalid/auth/authz/policy/rate/quota/transient/internal/timeout/connection/stream/schema/context/cancel/unknown classes while preserving Phase-1 compatibility aliases;
- retry semantics that remain bounded and retry only retry-safe classes;
- endpoint capability profiles with context/output/tool/parallel-tool/streaming/multimodal/cache/reasoning/cancellation flags;
- explicit privacy, retention, region and credential eligibility filtering before optimization;
- timestamped integer pricing snapshots and overflow-safe, round-up cost estimation;
- endpoint health state, transient-failure circuit breaking, rate-limit state and local quota reservation;
- deterministic `economy`, `balanced`, and `maximum-quality` routing policies that optimize only after hard eligibility filters;
- explicit model-snapshot pinning and stale-capability rejection;
- scout routing for sufficiently uncertain tasks without hard-coding model names into policy;
- bounded fallback across distinct eligible endpoints after endpoint-specific failures;
- attempt-level routing/fallback trace containing selected endpoint, strategy, expected cost, gateway attempts and outcome;
- ProviderBench/RouterBench deterministic tests with no live credentials or paid APIs;
- permanent Linux `Provider resilience + cost router` CI gate.

### Step-11 invariants

- security/privacy/capability constraints are filters, never utility penalties that a cheaper model can override;
- stale capability data fails closed for routing eligibility;
- a provider authentication failure is never blindly retried against the same endpoint;
- retries and failovers are separately bounded;
- rate-limit reservations cannot oversubscribe a known local quota window;
- cost arithmetic uses integer micro-USD accounting and never silently undercounts fractional token charges;
- circuit health is endpoint-scoped rather than provider-global;
- fallback re-runs eligibility and excludes the failed endpoint;
- deterministic routing tie-breaks on stable endpoint identity;
- core correctness tests require no live provider account.

## Step 11 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Expanded normalized failure taxonomy | IMPLEMENTED | `aer-provider::ProviderFailureClass`. |
| Retry-safe vs non-retry-safe semantics | IMPLEMENTED | gateway retry predicate + unit tests. |
| Capability/privacy/region/snapshot eligibility | IMPLEMENTED | `routing::eligibility`. |
| Capability freshness/drift fails closed | IMPLEMENTED | capability TTL + stale-profile test. |
| Timestamped pricing + exact integer cost accounting | IMPLEMENTED | `PricingSnapshot::estimate_cost_micros`. |
| Local rate-limit reservation | IMPLEMENTED | `RateLimitWindow::reserve`. |
| Endpoint-scoped health + circuit breaker | IMPLEMENTED | `EndpointHealth` + circuit test. |
| Deterministic economy/balanced/maximum-quality routing | IMPLEMENTED | `route` + RouterBench. |
| Scout routing under uncertainty | IMPLEMENTED | `ScoutThenRoute` decision test. |
| Bounded gateway retry | IMPLEMENTED | existing gateway + expanded taxonomy tests. |
| Bounded provider failover | IMPLEMENTED | `ResilientProviderPool` + ProviderBench. |
| Inspectable routing/fallback attempt trace | IMPLEMENTED | `ProviderAttemptRecord`. |
| No live paid API dependency in correctness gates | PASS BY DESIGN | scripted provider fixtures only. |
| CLI/TUI freeze preserved | PASS | no `crates/aer-cli/**` Step-11 changes. |
| Workspace format | PENDING | PR CI required. |
| Workspace `-D warnings` Clippy | PENDING | PR CI required. |
| Full workspace regression suite | PENDING | PR CI required. |
| ProviderBench + RouterBench Linux gate | PENDING | PR CI required. |
| Canonical isolated Windows CI verifier | PENDING | PR CI required. |
| Target Windows canonical verifier | PENDING | user reproduction required after merge. |

## Step 11 exit condition

Do not mark Step 11 COMPLETE or start Step 12 until:

1. the final Step-11 code tree passes workspace format, `-D warnings` Clippy, full tests, ProviderBench/RouterBench and the canonical Windows CI verifier;
2. temporary repair scaffolding, if any, has been removed;
3. the verified branch is merged to `main`;
4. the target Windows checkout runs `scripts/verify-windows.ps1` successfully and ends with `everything Windows verification: PASS`.
