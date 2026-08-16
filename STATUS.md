# everything Implementation Status

**Last updated:** 2026-08-16  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 3 — Repository Intelligence and Context Economy  
**Current step:** 09 / 18 — Context Economy Engine  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-09 code HEAD:** `0591452f13e2ba693d677e29b49d43e624fc175e`  
**Verified Step-09 CI:** `foundation-ci` run `31920562037` — Ubuntu PASS including permanent Context Economy gate; canonical isolated Windows verifier PASS  
**Next step:** 10 — Verification + Proof System — BLOCKED until Step-09 target Windows verification passes

## Agent engineering policy

`AGENTS.md` is the canonical implementation temperament for coding agents. YAGNI, semantic DRY, dependency restraint, bounded resource use, fail-closed correctness, evidence-before-completion, and measured performance apply to all remaining implementation work. `CLAUDE.md` delegates to it rather than duplicating policy.

## User-directed product-surface freeze

The CLI/TUI is intentionally frozen while the core architecture is completed. Until the user explicitly lifts this rule:

- do not add or redesign CLI/TUI features;
- do not expose new core capabilities through `crates/aer-cli`;
- do not use presentation work as a Step exit criterion;
- preserve the existing zero-redraw CLI only as a regression surface;
- develop and verify domain/core/storage/repository/context/runtime architecture first.

`crates/aer-cli/**` was not modified by Step 09.

## Completed milestones

- **Step 01 — Foundation Bootstrap:** COMPLETE — CI `31899011790`.
- **Step 02 — Executable Contract System:** COMPLETE — CI `31903313314`; target Windows PASS.
- **Phase 0:** COMPLETE.
- **Step 03 — Durable State Kernel:** COMPLETE — CI `31905250522`; target Windows PASS.
- **Step 04 — Runtime State + Resource Safety:** COMPLETE — CI `31906368065`; target Windows PASS.
- **Step 05 — Workspace + Execution Boundary:** COMPLETE — CI `31909059844`; target Windows PASS.
- **Step 06 — Single-Agent Runtime 0.1:** COMPLETE — CI `31911224304`; target Windows PASS.
- **Step 07 — Intent + Research + Engineering IR:** COMPLETE — semantic baseline `d5668b5d87a3b8a3f598b9cd016cc11cc5504837`; target Windows reproduction confirmed.
- **Step 08 — Repository Intelligence:** COMPLETE — code HEAD `12b97c6e9c715a19354af6ba5b661eb83ed9f353`; CI `31918025079`; target Windows canonical verification reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.

## Step 09 — Context Economy Engine

**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING

### Ownership and scope

New crate `aer-context` owns bounded, provenance-preserving Context Pack compilation. It consumes current Engineering IR plus derived repository intelligence and produces source-faithful context for a specific task. It is not an authority store and cannot alter accepted project semantics.

`aer-core::context::ContextService` is the application boundary. It binds a request to:

- the current authoritative Engineering IR revision;
- the exact current repository snapshot;
- the current repository-derived index;
- an explicit context policy;
- a hard input-token budget.

A stale Engineering IR revision or stale repository snapshot fails closed.

### Context objective

The engine optimizes for useful evidence per context cost rather than growing context. It combines bounded signals from:

- lexical/symbol repository retrieval;
- Engineering IR semantic links;
- repository structural/impact relationships;
- explicitly supplied runtime hints.

Initial fusion is deterministic rank-based scoring. A single repository path is one redundancy group: multiple signals enrich the same candidate rather than duplicating source content.

### Exact source identity

Step 09 added `RepositoryIndex::file(snapshot_id, path)` for exact snapshot-file lookup. Semantic, runtime, and structural paths use this direct identity lookup rather than running a lexical search to rediscover a known path.

Selected source is checked against its indexed full-file SHA-256 before use. Context segments carry exact line ranges and segment SHA-256. Fidelity verification rechecks both full-file and segment identity.

### Progressive disclosure

Context has four explicit tiers:

0. identifier/path only;
1. structural/anchor evidence;
2. bounded source span;
3. bounded expanded neighborhood.

Selection begins cheaply and upgrades already-selected items only while budget remains. Source code is extractive in Step 09; no abstractive code summary is introduced, avoiding unverifiable summary drift.

### Hard bounds and accounting

`ContextPolicy` bounds candidate count, selected items, source bytes, span size, semantic IDs, runtime hints, impact seeds, omitted-high-rank reporting, and selection weights.

`ContextRequest` carries an explicit input-token budget. The V1 accounting estimator is deliberately deterministic and conservative rather than pretending to be an exact provider tokenizer: one accounting unit per Unicode scalar plus fixed pack/item overhead. Provider/model-specific tokenizer accounting can be added only when a real provider capability requires it and measured evidence justifies the complexity.

Mandatory semantic coverage cannot silently disappear. If a required semantic ID has no resolvable current source, or its minimum context cannot fit the budget, compilation fails explicitly.

### Context Pack contract

Compiled packs bind:

- task ID;
- Engineering IR revision;
- exact repository snapshot;
- policy version;
- input-token budget;
- selected items and tiers;
- source references;
- full source hashes;
- exact segment hashes;
- selected reasons;
- omitted high-rank candidates.

Pack/item identities are deterministic SHA-256 identities. Every produced pack is validated through the existing executable `ContextPack` contract registry before it is returned.

### ContextBench

Step-09 tests compare bounded selection against a deliberately noisy whole-context fixture containing relevant auth implementation/tests plus large irrelevant documents.

The verified fixture requires:

- 100% relevant-path recall for its declared relevant set;
- fewer selected accounting tokens than naive whole context;
- higher relevant-evidence yield per token than the naive baseline;
- provenance/hash fidelity;
- hard token-budget compliance;
- no duplicate repository-path items.

Additional adversarial tests cover stale workspace refusal, stale IR refusal, missing mandatory semantic coverage, tiny-budget rejection, and exact file lookup independent from lexical search.

### YAGNI decisions

Step 09 deliberately did **not** add:

- a vector database;
- embedding infrastructure;
- provider-specific tokenizers;
- learned context routing;
- abstractive source-code summarization;
- a background context daemon;
- another persistent authority store.

Those components must earn their complexity through a demonstrated later requirement or measured regression.

## Step 09 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Dedicated `aer-context` ownership | PASS | workspace crate + core application boundary. |
| Current Engineering IR revision binding | PASS | `ContextService` stale-IR test. |
| Exact repository snapshot binding | PASS | `search_current` + final snapshot/fidelity checks. |
| Exact known-path identity without lexical rediscovery | PASS | `RepositoryIndex::file` + hardening test. |
| Lexical/symbol + semantic + structural + runtime signal fusion | PASS | Context Engine integration fixture. |
| Repository-path redundancy elimination | PASS | ContextBench unique-path assertion. |
| Mandatory semantic coverage | PASS | positive + missing-coverage tests. |
| Hard context/token budgets | PASS | policy validation + tiny-budget rejection. |
| Progressive disclosure tiers | PASS | deterministic tier compiler tests. |
| Extractive source spans with provenance | PASS | full-file + line-segment SHA-256. |
| Stale workspace/context rejection | PASS | stale-index/fidelity tests. |
| Current executable Context Pack schema validation | PASS | embedded contract validation on compile/fidelity. |
| Deterministic pack/item identities | PASS | SHA-256 identity construction. |
| ContextBench relevant recall | PASS | fixture recall `1000` milli. |
| ContextBench lower token cost than naive whole context | PASS | baseline assertion. |
| ContextBench higher relevant yield/token than naive baseline | PASS | baseline assertion. |
| No new third-party Context Engine dependency | PASS | activation lockfile adds only internal `aer-context` package edge. |
| Workspace-wide format | PASS | CI `31920562037`. |
| Workspace-wide `-D warnings` Clippy | PASS | CI `31920562037`. |
| Full workspace regression suite | PASS | CI `31920562037`. |
| Permanent Linux Context Economy CI gate | PASS | CI `31920562037`. |
| Canonical isolated Windows CI verifier including `aer-context` | PASS | CI `31920562037`. |
| Temporary write workflow/repair scaffolding removed | PASS | verified code HEAD `0591452f13e2ba693d677e29b49d43e624fc175e`; repository workflow is read-only again. |
| Target Windows canonical verifier | PENDING | user reproduction required. |

## Step 09 exit condition

Repository-side Step 09 is verified. Do **not** start Step 10 until the target Windows checkout reproduces the canonical verifier successfully.

No interactive CLI testing is required. Run only:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
```

Expected final line:

```text
everything Windows verification: PASS
```

After that PASS, mark Step 09 COMPLETE and proceed to **Step 10 — Verification + Proof System**, keeping the CLI/TUI frozen.
