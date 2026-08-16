from pathlib import Path

path = Path("STATUS.md")
text = path.read_text(encoding="utf-8")

old_header = """**Current phase:** Phase 5 — Provider Resilience + Cost Routing  
**Current step:** 11 / 18 — Provider Resilience + Cost Router  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-11 code HEAD:** `163ba7903e719cefcb3595f025f12358c376babe`  
**Verified Step-11 CI:** `foundation-ci` run `31951923261` — Ubuntu PASS including permanent Provider Resilience + Cost Router gate; canonical isolated Windows verifier PASS  
**Next step:** 12 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery — BLOCKED until Step-11 target Windows verification passes
"""
new_header = """**Current phase:** Phase 6 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery  
**Current step:** 12 / 18 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-12 code HEAD:** `5446fb2d4887d0bc7b18174af6cc81bd349a837c`  
**Verified Step-12 CI:** `foundation-ci` run `31957740270` — Ubuntu PASS including RepoIntelBench, Tier-2 topology and HandoffBench; canonical isolated Windows verifier PASS  
**Next step:** 13 — Bounded Parallel Execution — BLOCKED until Step-12 target Windows verification passes
"""
if old_header not in text:
    raise SystemExit("status header target missing")
text = text.replace(old_header, new_header, 1)

text = text.replace(
    "`crates/aer-cli/**` was not modified by Step 10 or Step 11.",
    "`crates/aer-cli/**` was not modified by Steps 10–12.",
    1,
)

milestone_anchor = "- **Step 10 — Verification + Proof System:** COMPLETE — repository CI `31939146224`; post-merge main CI `31939487328`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.\n"
if milestone_anchor not in text:
    raise SystemExit("Step 10 milestone anchor missing")
text = text.replace(
    milestone_anchor,
    milestone_anchor
    + "- **Step 11 — Provider Resilience + Cost Router:** COMPLETE — repository CI `31951923261`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.\n",
    1,
)

text = text.replace(
    "**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING",
    "**State:** COMPLETE",
    1,
)
text = text.replace(
    "| Target Windows canonical verifier | PENDING | user reproduction required on updated `main`. |",
    "| Target Windows canonical verifier | PASS | user reproduction on 2026-08-16; final line `everything Windows verification: PASS`. |",
    1,
)

old_tail_start = "## Step 11 exit condition\n\nRepository-side Step 11 is verified. Do **not** start Step 12 until the target Windows checkout reproduces the canonical verifier successfully.\n"
if old_tail_start not in text:
    raise SystemExit("Step 11 exit block target missing")
prefix = text.split(old_tail_start, 1)[0]
new_tail = r'''## Step 11 exit condition

Step 11 is closed. The target Windows checkout reproduced the canonical verifier on 2026-08-16; its acceptance evidence remains above for audit/replay.

## Step 12 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery

**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING

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
| Target Windows canonical verifier | PENDING | user reproduction required after Step 12 is merged to `main`. |

## Step 12 exit condition

Repository-side Step 12 is verified. Do **not** start Step 13 until the target Windows checkout reproduces the canonical verifier successfully.

No interactive CLI testing is required. After Step 12 is merged to `main`, run only:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
```

Expected final line:

```text
everything Windows verification: PASS
```

After that PASS, mark Step 12 COMPLETE and proceed to **Step 13 — Bounded Parallel Execution**, keeping the CLI/TUI frozen.
'''

path.write_text(prefix + new_tail, encoding="utf-8")
