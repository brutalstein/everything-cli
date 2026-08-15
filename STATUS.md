# everything Implementation Status

**Last updated:** 2026-08-16  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 2 — Intent, Research, and Engineering IR  
**Current step:** 07 / 18 — Intent + Research + Engineering IR  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction of the Step-07 HEAD  
**Verified Step-07 HEAD:** `d5668b5d87a3b8a3f598b9cd016cc11cc5504837`  
**Verified Step-07 CI:** `foundation-ci` run `31913483431` — Ubuntu PASS, canonical isolated Windows verifier PASS  
**Next step:** 08 — Repository Intelligence — BLOCKED until Step-07 target Windows verification passes

## Completed milestones

- **Step 01 — Foundation Bootstrap:** COMPLETE — CI `31899011790`.
- **Step 02 — Executable Contract System:** COMPLETE — CI `31903313314`; target Windows PASS.
- **Phase 0:** COMPLETE.
- **Step 03 — Durable State Kernel:** COMPLETE — canonical hardened CI `31905250522`; target Windows PASS.
- **Step 04 — Runtime State + Resource Safety:** COMPLETE — CI `31906368065`; target Windows PASS.
- **Step 05 — Workspace + Execution Boundary:** COMPLETE — CI `31909059844`; target Windows verifier and real interactive `everything.exe` launch confirmed by the user on 2026-08-16.
- **Step 06 — Single-Agent Runtime 0.1:** COMPLETE — repository CI `31911224304`; target Windows canonical verifier, headless runtime checks, and real interactive TUI launch confirmed by the user on 2026-08-16.

## Product-surface rule

Backbone first, TUI in parallel. Domain/runtime/storage/execution semantics remain authoritative. The TUI and headless CLI project the same application APIs and durable state; they may not duplicate business logic, fabricate unavailable capabilities, or promote presentation state into authority.

## Terminal interaction model

The interactive product now follows the architecture's conversation-plus-semantic-progress model with a **persistent bottom composer**.

Current interaction behavior:

- the composer is always rendered at the bottom of the TUI;
- ordinary text is accepted as user intent and persisted through `SpecService`;
- slash commands are the primary explicit activation surface;
- arrow keys remain first-class for navigation, slash-command selection, composer cursor movement, and history;
- `Tab` / `Shift+Tab` cycle composer → navigation → content;
- `Esc` / `Ctrl+C` clear the composer or return to Home without silently cancelling durable work;
- `/quit` is the explicit exit command; ordinary `q` remains normal text;
- slash completion is prefix-based and deterministic; no unknown commands are invented.

Current slash commands:

```text
/home
/intent
/research
/ir
/workspace
/environment
/providers
/activity
/settings
/goal <statement>
/non-goal <statement>
/constraint <statement>
/accept <observable criterion>
/assumption <statement>
/quality <attribute>
/decision <choice>
/research-import <artifact.json>
/refresh
/clear
/help
/quit
```

`/providers` intentionally projects the real current provider state. The gateway exists, but no authenticated production provider profile/secure credential transport has been implemented yet, so the surface truthfully reports `not configured` instead of presenting a mock settings form.

## Material Symbols asset system

The previous generic Material-like glyph layer has been replaced by a source-backed asset system.

- canonical assets are vendored **Google Material Symbols Rounded** SVGs under `crates/aer-cli/assets/material-symbols/rounded/`;
- each asset records upstream symbol identity, upstream Git blob, and local SHA-256 provenance;
- the upstream Apache-2.0 license is vendored alongside the SVGs;
- terminal icons are mechanically derived compact 8×4 Braille raster projections of those SVGs rather than arbitrary decorative glyph choices;
- runtime asset-integrity checks verify SVG identity against recorded SHA-256 values;
- `EVERYTHING_ASCII=1` remains an explicit fallback and asset-integrity failure also prevents pretending a broken icon source is healthy;
- no Material/Nerd font installation is required and no binary font asset is part of the product dependency surface.

## Step 07 — Intent + Research + Engineering IR

**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING

### Deterministic domain semantics

`aer-domain::spec` now owns deterministic model-independent semantics for:

- source/provenance references;
- semantic status/risk/priority values;
- requirements and acceptance criteria;
- user/system/organization decisions with reversibility and confidence;
- explicit unknowns and resolution modes;
- deterministic question-value ordering;
- `IntentState`;
- `EngineeringIr`;
- monotonic `SpecDelta`;
- semantic checksum and severity.

The question policy ranks user questions from uncertainty × impact × irreversibility / friction with stable-ID tie breaking. Accepted semantics cannot silently disappear, change meaning under the same stable ID, or appear as unsupported accepted additions without the checksum becoming `High` severity.

### Durable specification application boundary

`aer-core::spec::SpecService` is the shared application boundary used by TUI and headless surfaces.

It supports:

- `inspect` of durable specification state;
- natural-language `submit_message` without fabricating unavailable model extraction;
- explicit user-authoritative semantic recording;
- explicit user decision recording;
- bounded ResearchArtifact ingestion;
- deterministic Engineering IR compilation;
- executable structural + semantic validation;
- semantic checksum gating before persistence;
- content-addressed persisted IR documents;
- monotonic revision numbers;
- durable `SpecDelta` records;
- replay from the existing event journal/CAS.

For a greenfield project, the first user request is preserved verbatim as a provenance-backed goal. The current deterministic minimum compiler then opens an explicit high-value unknown for observable completion criteria rather than guessing acceptance semantics. Explicit `/accept` input resolves that unknown and creates the next IR revision.

### Research authority boundary

`aer-research` accepts only schema-valid, bounded, claim-oriented ResearchArtifact data.

Hard bounds and integrity checks include artifact bytes, source count, claim count, URI/claim sizes, duplicate source/claim IDs, source-reference resolution, and non-empty source content identity.

External research is **evidence, not authority**:

- incoming non-empty `promoted_refs` are rejected;
- imported claims remain `ResearchFinding` values with source references/status/confidence;
- research cannot self-promote into accepted requirements or decisions;
- contradictions/insufficient claims remain representable rather than being collapsed into a fabricated conclusion;
- acquisition/network search remains a separate future adapter boundary, so Step 07 does not fake web results.

### TUI and headless projections

The TUI has real architecture-backed surfaces for:

- `/intent` — messages, goals, decisions, explicit unknowns and next high-value question;
- `/research` — source-backed imported research evidence only;
- `/ir` — current Engineering IR, semantic checksum, revision, and latest SpecDelta;
- `/providers`, `/activity`, `/workspace`, `/environment`, `/settings` — existing authoritative projections.

Headless equivalents are available from the same binary:

```text
everything status [--json]
everything doctor [--json]
everything intent [--json]
everything ir [--json]
everything research [--json]
everything runs [--json]
everything workspace [--json]
everything providers
```

## Step 07 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Stable source-backed greenfield IR | PASS | `aer-core::spec` greenfield E2E test. |
| Explicit high-value unknown/question | PASS | deterministic question-value policy + greenfield test. |
| Explicit acceptance resolves unknown | PASS | spec E2E creates revision 2 + SpecDelta. |
| Stable semantic IDs | PASS | SHA-256 based deterministic ID derivation + replay test. |
| Structural Engineering IR validation | PASS | embedded Draft-2020-12 contract registry. |
| Semantic Engineering IR validation | PASS | executable semantic validator. |
| Material omission/distortion blocked | PASS | semantic checksum tests. |
| Unsupported accepted additions blocked | PASS | semantic checksum tests. |
| Monotonic IR revisions + SpecDelta | PASS | durable compilation tests. |
| Research artifact budgets/integrity | PASS | `aer-research` tests. |
| Research cannot self-promote to authority | PASS | research authority tests + core integration test. |
| TUI uses authoritative spec state | PASS | `everything` product tests. |
| Persistent composer + slash command parser | PASS | TUI/parser/render tests. |
| Arrow-key navigation/history/slash selection | PASS | deterministic AppState tests. |
| Official Material SVG asset integrity | PASS | asset SHA-256 test. |
| Linux format + `-D warnings` Clippy + full workspace tests | PASS | `31913483431`. |
| Dedicated Intent + Research + Engineering IR gate | PASS | `31913483431`. |
| Existing runtime/workspace/storage/contracts regressions | PASS | `31913483431`. |
| Canonical isolated Windows CI verifier | PASS | `31913483431`. |
| Target Windows Step-07 verifier + interactive launch | PENDING | Pull current `main`, run verifier, launch `everything.exe`, exercise slash surfaces. |

## Step 07 exit condition

Repository-side Step-07 gates are satisfied. Do **not** start Step 08 until the target Windows checkout reproduces the current verifier and the updated product launches successfully.

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
& ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
```

Inside the TUI, exercise at least:

```text
/help
/providers
/intent
/ir
/research
/activity
/workspace
```

For a real local Step-07 smoke test, enter a natural request, inspect `/intent`, then explicitly provide an observable acceptance criterion with `/accept <criterion>` and inspect `/ir` again. This must create durable real specification state; no mock/sample data is injected.

Useful headless checks:

```powershell
$everything = ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
& $everything status
& $everything intent
& $everything ir
& $everything research
& $everything doctor
```

A final `everything Windows verification: PASS` plus a successful interactive Step-07 smoke test closes Step 07 and makes Step 08 READY.
