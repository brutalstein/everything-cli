# everything Implementation Status

**Last updated:** 2026-08-16  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 2 — Intent, Research, and Engineering IR  
**Current step:** 07 / 18 — Intent + Research + Engineering IR  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction of the current product HEAD  
**Verified current product HEAD:** `af192072e389c87e5b83d7e61470a04ac6b80d16`  
**Verified current CI:** `foundation-ci` run `31915577541` — Ubuntu PASS, canonical isolated Windows verifier PASS  
**Step-07 semantic implementation baseline:** `d5668b5d87a3b8a3f598b9cd016cc11cc5504837`  
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

## Terminal UX redesign — repository verified

The permanent multi-surface dashboard/sidebar has been removed from the normal interactive path. The product now has a two-stage terminal interaction model.

### Stage 1 — local control-plane / daemon launcher

A bare interactive invocation of `everything` first opens a local control-plane launcher instead of assuming the current directory is already the desired workspace.

The launcher:

- restores the premium `everything` ASCII wordmark and the cyan/violet product palette;
- identifies the current runtime truthfully as an embedded local runtime rather than inventing a background daemon that does not yet exist;
- exposes `Open current repository`, `Choose another repository`, and `Quit`;
- accepts typed/pasted absolute or relative paths, including `~` expansion where available;
- validates the selected path through the existing `WorkspaceIdentity` boundary;
- only attaches a valid Git working tree and does not mutate an arbitrary folder merely to make the launcher succeed;
- reports the production provider profile as `not configured` until the secure provider onboarding transport actually exists;
- supports arrow selection, Enter, editable path input, Esc, and Ctrl+C.

This launcher is presentation over the existing local runtime/workspace architecture. It is intentionally not a fake long-running service.

### Stage 2 — conversation-first workspace

After a real repository workspace is selected, the interface switches to a Claude-Code-like conversation-first shell rather than preserving the launcher/dashboard chrome.

The workspace surface now has:

- compact workspace/branch/clean-state header;
- transcript-first center area;
- restored premium `everything` wordmark on an empty conversation;
- persistent bottom composer using a visible `❯` prompt;
- persistent compact status line with workspace, branch, clean/dirty state, Engineering IR revision, durable run count, and provider state;
- no permanent navigation sidebar;
- slash-command suggestions only while typing `/`;
- `/intent`, `/research`, `/ir`, `/workspace`, `/environment`, `/providers`, `/activity`, and `/settings` as temporary detail surfaces rather than permanent panes;
- `Esc` returning detail surfaces to the conversation;
- Up-arrow history recall and slash-selection behavior without hidden navigation changes;
- `Tab` / `Shift+Tab` kept on the composer so no invisible sidebar focus state can capture input;
- natural text staying in the conversation after it is persisted through the authoritative `SpecService` boundary;
- explicit `/quit` exit rather than single-letter shortcut ambiguity.

The existing headless CLI remains available for scripts and automation. Only the bare interactive TTY path uses the two-stage launcher/workspace shell.

## Material Symbols asset system

The canonical icon sources remain vendored **Google Material Symbols Rounded** SVGs under `crates/aer-cli/assets/material-symbols/rounded/`.

- every source asset records upstream symbol identity, upstream Git blob, and local SHA-256 provenance;
- the upstream Apache-2.0 license is vendored alongside the SVGs;
- runtime/tests verify the embedded SVG bytes against their recorded SHA-256 identity;
- terminal rendering no longer depends on the previous tiny Braille raster masks, which proved visually unreliable across terminal/font combinations;
- the TUI uses larger terminal-safe semantic Unicode projections such as `▣`, `◉`, `⚙`, `▲`, `◆`, and `→` while retaining the verified Material SVG as the canonical source/provenance asset;
- `EVERYTHING_ASCII=1` remains the explicit lowest-common-denominator fallback;
- no Material/Nerd font installation and no redistributed font binary is required.

A terminal cannot natively render arbitrary SVG inside a normal text cell, so the terminal projection is deliberately separate from the canonical SVG source. The UI does not claim that a Unicode cell is the SVG itself.

## Current slash-command surface

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

## Step 07 — Intent + Research + Engineering IR

**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING

### Deterministic domain semantics

`aer-domain::spec` owns deterministic model-independent semantics for:

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

## Step 07 + terminal UX acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Stable source-backed greenfield IR | PASS | `aer-core::spec` greenfield E2E test. |
| Explicit high-value unknown/question | PASS | deterministic question-value policy + greenfield test. |
| Explicit acceptance resolves unknown | PASS | spec E2E creates revision 2 + SpecDelta. |
| Stable semantic IDs | PASS | SHA-256 based deterministic ID derivation + replay test. |
| Structural + semantic Engineering IR validation | PASS | embedded contract registry + semantic validator. |
| Material omission/distortion or unsupported accepted addition blocked | PASS | semantic checksum tests. |
| Monotonic IR revisions + SpecDelta | PASS | durable compilation tests. |
| Research artifact budgets/integrity | PASS | `aer-research` tests. |
| Research cannot self-promote to authority | PASS | research authority tests + core integration test. |
| Bare interactive launch uses workspace launcher | PASS | launcher/entry implementation + terminal product build. |
| Launcher validates real Git workspace identity | PASS | `WorkspaceIdentity` boundary; no mock workspace data. |
| Workspace shell has no permanent sidebar | PASS | terminal render test asserts the old `SURFACES` chrome is absent. |
| Persistent composer remains visible on narrow/wide terminals | PASS | terminal render tests. |
| Slash views are temporary detail surfaces | PASS | render tests + command mapping. |
| Hidden Tab/sidebar focus removed | PASS | workspace entry test. |
| Up-arrow history works without hidden navigation | PASS | workspace entry test. |
| Official Material SVG asset integrity | PASS | asset SHA-256 test. |
| Linux format + `-D warnings` Clippy + full workspace tests | PASS | `31915577541`. |
| Dedicated Intent + Research + Engineering IR gate | PASS | `31915577541`. |
| Dedicated terminal product surface gate | PASS | `31915577541`. |
| Existing runtime/workspace/storage/contracts regressions | PASS | `31915577541`. |
| Canonical isolated Windows CI verifier | PASS | `31915577541`. |
| Target Windows current product verifier + interactive UX smoke | PENDING | Pull current `main`, run verifier, exercise launcher and workspace shell. |

## Step 07 exit condition

Repository-side Step-07 and terminal-redesign gates are satisfied. Do **not** start Step 08 until the target Windows checkout reproduces the current verifier and the redesigned product launches successfully.

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
& ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
```

Interactive smoke expectations:

1. A bare launch first shows the local control-plane/daemon launcher and the restored `everything` wordmark.
2. Use arrows + Enter to open the current Git repository, or choose another real Git repository path.
3. The workspace view must have no permanent sidebar; the main surface is conversation + bottom composer + status line.
4. Type `/` and confirm the slash-command menu appears only on demand.
5. Exercise `/providers`, `/intent`, `/ir`, `/research`, `/activity`, and `/workspace`; `Esc` should return to chat.
6. Enter a natural request. It must remain in the conversation and persist through the real specification boundary.
7. If an acceptance unknown is present, use `/accept <criterion>` and inspect `/ir` again.

Useful headless checks remain:

```powershell
$everything = ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
& $everything status
& $everything intent
& $everything ir
& $everything research
& $everything doctor
```

A final `everything Windows verification: PASS` plus a successful redesigned interactive smoke test closes Step 07 and makes Step 08 READY.
