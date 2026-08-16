# everything Implementation Status

**Last updated:** 2026-08-16  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 3 — Repository Intelligence and Context Economy  
**Current step:** 08 / 18 — Repository Intelligence  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-08 HEAD:** `12b97c6e9c715a19354af6ba5b661eb83ed9f353`  
**Verified Step-08 CI:** `foundation-ci` run `31918025079` — Ubuntu PASS, canonical isolated Windows verifier PASS  
**Next step:** 09 — Context Economy Engine — BLOCKED until Step-08 target Windows verification passes

## User-directed product-surface freeze

The CLI/TUI is intentionally frozen while the core architecture is completed. Until the user explicitly lifts this rule:

- do not add or redesign CLI/TUI features;
- do not expose new core capabilities through `crates/aer-cli`;
- do not use presentation work as a Step exit criterion;
- preserve the existing zero-redraw CLI only as a regression surface;
- develop and verify domain/core/storage/repository/context/runtime architecture first.

`crates/aer-cli/**` was not modified by Step 08.

## Completed milestones

- **Step 01 — Foundation Bootstrap:** COMPLETE — CI `31899011790`.
- **Step 02 — Executable Contract System:** COMPLETE — CI `31903313314`; target Windows PASS.
- **Phase 0:** COMPLETE.
- **Step 03 — Durable State Kernel:** COMPLETE — CI `31905250522`; target Windows PASS.
- **Step 04 — Runtime State + Resource Safety:** COMPLETE — CI `31906368065`; target Windows PASS.
- **Step 05 — Workspace + Execution Boundary:** COMPLETE — CI `31909059844`; target Windows PASS.
- **Step 06 — Single-Agent Runtime 0.1:** COMPLETE — CI `31911224304`; target Windows PASS.
- **Step 07 — Intent + Research + Engineering IR:** COMPLETE — semantic baseline `d5668b5d87a3b8a3f598b9cd016cc11cc5504837`; repository and target-Windows product/semantic reproduction confirmed before Step 08 began.

## Step 08 — Repository Intelligence

**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING

### Architecture boundary

`aer-repo` owns derived, rebuildable repository intelligence. It is not an authority store and may not change project intent, decisions, Engineering IR, runtime truth, or the user's workspace.

`aer-core::repository::RepositoryService` is the application boundary that binds repository intelligence to an exact workspace snapshot and current Engineering IR. Derived index state is stored outside the user repository under the project runtime state directory.

### Exact repository snapshot identity

Repository retrieval is tied to an exact snapshot identity derived from:

- repository identity;
- HEAD commit;
- tracked dirty-diff SHA-256;
- aggregate **actual untracked-content SHA-256** identity;
- submodule-state SHA-256.

The workspace is captured before and after indexing. If it changes during the build, indexing fails with `WorkspaceChangedDuringIndex`; an incomplete build never becomes current.

`search_current` fails closed with `StaleIndex` when the current workspace no longer matches the indexed snapshot. There is no nearby-commit or best-effort silent reuse.

### Derived index and bounded inventory

The repository index is a separate SQLite database configured with WAL, FULL synchronous durability, foreign keys enabled, `trusted_schema=OFF`, busy timeout, and an explicit derived-index schema version.

Inventory is ignore-aware and deterministic through:

```text
git ls-files --cached --others --exclude-standard -z
```

Configured hard bounds cover:

- repository file count;
- per-file text bytes;
- aggregate text bytes;
- terms per file;
- syntax links per file;
- Git commit history;
- co-change fanout;
- Git command output capture;
- query bytes;
- returned results;
- retained snapshots.

Limit violations fail closed rather than silently expanding resource use.

### Incremental content reuse

Parsed artifacts are keyed by:

```text
content_sha256 + parser_key
```

Unchanged file content is reused across repository snapshots only when both identities match. Parser identity is part of the key, so changing a parser version or adapter identity invalidates reuse deterministically.

Snapshot retention cannot delete the current-snapshot pointer. Unreferenced derived artifacts are garbage-collected only after snapshot transitions commit.

### Syntax and language adapters

Pinned Tree-sitter support exists for:

- Rust;
- Python;
- JavaScript;
- TypeScript;
- TSX.

The adapters derive bounded symbols and import/call/reference relations. Unsupported text formats use deterministic lexical indexing without fabricating syntax symbols.

Parser-specific tests cover Rust, Python, JavaScript, TypeScript, and unsupported-language lexical fallback.

### Retrieval

Repository retrieval currently combines:

- deterministic lexical tokenization;
- BM25-style file scoring;
- exact symbol-name boosts;
- path proximity signal;
- symbol anchor lines;
- explicit score thresholds;
- bounded result count.

When evidence is absent or below threshold, retrieval returns an explicit abstention reason rather than inventing a plausible file.

No vector database or embedding dependency was added in Step 08. That remains optional and must earn its complexity through measured Step-09 context-retrieval pressure.

### Repository graph views

The derived index exposes:

- symbols;
- imports;
- calls;
- references;
- test-to-code associations;
- bounded Git history;
- file co-change relationships;
- semantic links from current Engineering IR anchors;
- runtime-observation link ingestion for observations that actually reference files in the exact snapshot;
- impact candidates combining tests, co-change, and lexical/reference proximity.

Runtime observations for paths absent from the exact snapshot are ignored instead of creating fabricated repository edges.

### Engineering IR linkage

`RepositoryService::refresh` reads the current authoritative specification state and creates derived semantic anchors for existing:

- goals;
- functional requirements;
- constraints;
- acceptance criteria;
- decisions.

These are retrieval links only. Repository intelligence cannot promote derived evidence back into accepted semantic authority.

### Retrieval baseline

`aer-repo` contains a deterministic retrieval-evaluation API that reports case count, relevant-path count, relevant paths found, and recall in milli-units. Step-08 tests exercise a small source/test fixture and require the relevant auth implementation and associated test to be recovered.

## Step 08 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Exact commit/dirty/untracked/submodule snapshot identity | PASS | `RepoSnapshotIdentity` + snapshot capture tests. |
| Ignore-aware deterministic inventory | PASS | bounded `git ls-files --cached --others --exclude-standard -z`. |
| Workspace changed during indexing cannot publish current index | PASS | before/after snapshot comparison + transactional current pointer. |
| Stale index cannot be silently reused | PASS | `search_current` returns `StaleIndex`. |
| Content-hash + parser-key incremental reuse | PASS | incremental refresh test. |
| Rust Tree-sitter adapter | PASS | syntax adapter test. |
| Python Tree-sitter adapter | PASS | syntax adapter test. |
| JavaScript Tree-sitter adapter | PASS | syntax adapter test. |
| TypeScript/TSX Tree-sitter adapter | PASS | syntax adapter test. |
| Unsupported language has lexical fallback without fake symbols | PASS | fallback test. |
| Lexical/symbol retrieval with explicit abstention | PASS | retrieval tests. |
| Query/result/resource budgets fail closed | PASS | policy validation + adversarial oversized-query test. |
| Import/call/reference graph | PASS | parsed-content link index and APIs. |
| Test relationships | PASS | source/test integration fixture. |
| Bounded Git history + co-change view | PASS | multi-commit fixture. |
| Semantic IR-to-repository links | PASS | `RepositoryService` integration test. |
| Runtime observation port does not fabricate missing-file links | PASS | adversarial runtime-observation test. |
| Snapshot retention preserves current pointer | PASS | adversarial retention test. |
| Impact view | PASS | tests + co-change + lexical/reference composition. |
| Retrieval baseline metrics | PASS | deterministic evaluation fixture. |
| Derived index lives outside user workspace | PASS | core integration test. |
| Workspace-wide format + `-D warnings` Clippy | PASS | CI `31918025079`. |
| Full workspace regression suite | PASS | CI `31918025079`. |
| Dedicated repository-intelligence gate | PASS | CI `31918025079`. |
| Canonical isolated Windows CI verifier with `aer-repo` | PASS | CI `31918025079`. |
| Target Windows verifier | PENDING | user reproduction required. |

## Step 08 exit condition

Repository-side Step 08 is verified. Do **not** start Step 09 until the target Windows checkout reproduces the canonical verifier successfully.

Run only the architecture verification; no interactive CLI testing is required for this step:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
```

Expected final line:

```text
everything Windows verification: PASS
```

After that PASS, mark Step 08 COMPLETE and proceed to **Step 09 — Context Economy Engine** while keeping the CLI/TUI frozen.
