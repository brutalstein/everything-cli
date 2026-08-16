# everything Implementation Status

**Last updated:** 2026-08-16  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 2 — Intent, Research, and Engineering IR  
**Current step:** 07 / 18 — Intent + Research + Engineering IR  
**Repository-side state:** ZERO-REDRAW CLI CI VERIFIED — awaiting target Windows reproduction  
**Verified product HEAD:** `24204c9fc735bb73776f7545394307d6dd5bd377`  
**Verified repository CI:** `foundation-ci` run `31916597879` — Ubuntu PASS including zero-redraw guard; canonical isolated Windows verifier PASS  
**Step-07 semantic implementation baseline:** `d5668b5d87a3b8a3f598b9cd016cc11cc5504837`  
**Zero-redraw CLI implementation baseline:** `c1980a7d77144bf8b80ba8bcf8cb8d2b382816a0`  
**Verified rebuild finalizer:** `31916419771` — format PASS, workspace check PASS, `-D warnings` Clippy PASS, full workspace tests PASS  
**Next step:** 08 — Repository Intelligence — BLOCKED until target Windows reproduction passes

## Completed milestones

- **Step 01 — Foundation Bootstrap:** COMPLETE — CI `31899011790`.
- **Step 02 — Executable Contract System:** COMPLETE — CI `31903313314`; target Windows PASS.
- **Phase 0:** COMPLETE.
- **Step 03 — Durable State Kernel:** COMPLETE — canonical hardened CI `31905250522`; target Windows PASS.
- **Step 04 — Runtime State + Resource Safety:** COMPLETE — CI `31906368065`; target Windows PASS.
- **Step 05 — Workspace + Execution Boundary:** COMPLETE — CI `31909059844`; target Windows verifier and real interactive launch confirmed by the user on 2026-08-16.
- **Step 06 — Single-Agent Runtime 0.1:** COMPLETE — repository CI `31911224304`; target Windows canonical verifier, headless runtime checks, and interactive launch confirmed by the user on 2026-08-16.

## Product-surface rule

Domain/runtime/storage/workspace/execution/specification semantics are authoritative. The CLI is only a projection over those application boundaries. It may not duplicate business logic, fabricate unavailable capabilities, or turn presentation state into authority.

## Full-screen TUI retired

The previous Ratatui/Crossterm product surface has been deleted rather than hidden or optimized in place.

Removed from `crates/aer-cli`:

- `app.rs`;
- `entry.rs`;
- `launcher.rs`;
- `ui.rs`;
- `theme.rs`;
- `slash.rs`;
- `material_icons.rs`;
- the complete terminal Material-symbol asset tree.

The `everything` package no longer depends on `ratatui`, `crossterm`, or the old CLI-only `sha2` asset-integrity dependency. Regenerating the lockfile after that removal eliminated the retired terminal rendering dependency subtree.

This is now an explicit performance invariant. CI fails if Ratatui/Crossterm returns to the `everything` dependency tree or if retired TUI source/assets reappear.

## Current interactive product — zero-redraw line shell

A bare interactive invocation starts a blocking line-oriented shell:

```text
everything
workspace <selected path>
type /help for available commands

❯
```

The shell has no alternate terminal screen and no frame renderer. While waiting for input it blocks in `stdin.read_line()`; it does not poll, animate, refresh, or redraw on individual keypresses.

### Startup performance contract

Before the user requests work, interactive startup performs only lightweight process work:

1. Clap argument parsing;
2. current-directory resolution;
3. optional `--workspace PATH` path resolution;
4. TTY detection;
5. three startup lines and the prompt.

It deliberately does **not** call any of these before the first requested operation:

- `WorkspaceIdentity::inspect`;
- `EnvironmentFingerprint::discover`;
- `list_runs`;
- `SpecService::inspect` or specification replay.

The startup path therefore does not hash Git dirty state, enumerate runtime history, replay durable specification state, probe toolchains, build a render tree, process per-key terminal events, or validate presentation assets.

### Lazy cost model

Heavy work is paid only by the command that needs it:

- `/workspace` → authoritative `WorkspaceIdentity` inspection;
- `/status` → workspace identity + durable runtime catalog + specification state, **without** environment fingerprinting;
- `/intent`, `/ir`, `/research` → specification state only;
- `/runs` → durable runtime ledger;
- `/doctor` → explicit full diagnostic including `EnvironmentFingerprint`;
- ordinary text and semantic write commands → the authoritative `SpecService` mutation/compilation path.

`/doctor` is therefore expected to be materially heavier than shell startup; that cost is intentional and user-requested rather than hidden in every launch.

## Current interactive command surface

Only capabilities backed by current implementation are advertised:

```text
/status
/workspace
/intent
/ir
/research
/runs
/providers
/doctor

/goal <text>
/non-goal <text>
/constraint <text>
/accept <text>
/assumption <text>
/quality <text>
/decision <text>
/research-import <artifact.json>

/help
/quit
```

There is no `/home`, `/settings`, `/activity`, `/environment`, `/clear`, launcher screen, command palette, icon navigation, fake daemon screen, or provider-login UI.

### Actual authority behind commands

- ordinary text → `SpecService::submit_message`;
- semantic statements → `SpecService::record_semantic` with explicit `UserSemanticKind`;
- user decision → `SpecService::record_user_decision`;
- research import → `SpecService::ingest_research` after reading the supplied JSON artifact;
- intent/IR/research reads → `SpecService::inspect`;
- runtime history → `list_runs`;
- repository identity → `WorkspaceIdentity::inspect`;
- environment/tool fingerprint → `EnvironmentFingerprint::discover` only from `doctor`;
- provider surface → current provider-gateway implementation state only; production auth profile remains not configured because onboarding is not implemented yet.

Research acquisition/network search is still not implemented in Step 07. `/research-import` ingests an already acquired, validated ResearchArtifact and cannot invent web research.

## Headless CLI

The same binary retains scriptable commands:

```text
everything [--workspace PATH] status [--json]
everything [--workspace PATH] workspace [--json]
everything [--workspace PATH] intent [--json]
everything [--workspace PATH] ir [--json]
everything [--workspace PATH] research [--json]
everything [--workspace PATH] runs [--json]
everything [--workspace PATH] providers
everything [--workspace PATH] doctor [--json]
```

When stdin/stdout are not TTYs and no subcommand is supplied, the product emits plain status instead of entering an interactive shell.

## Step 07 semantic state

The zero-redraw shell did not replace Step-07 domain/application semantics. Existing verified behavior remains:

- deterministic source/provenance-backed `IntentState`;
- user-authoritative goals, constraints, assumptions, quality attributes, acceptance criteria and decisions;
- explicit unknowns and deterministic next-question ranking;
- source-backed ResearchArtifact ingestion with external-evidence authority only;
- deterministic Engineering IR compilation;
- structural and semantic contract validation;
- semantic checksum gates;
- monotonic Engineering IR revisions and `SpecDelta`;
- durable event/CAS replay through the existing storage kernel.

The product surface only invokes these existing boundaries more cheaply.

## Zero-redraw CLI acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Previous full-screen TUI source removed | PASS | `7ebb6fa3b27e4bcc402050828c0092d3e85dd37d`. |
| Material/icon presentation assets removed | PASS | `7ebb6fa3b27e4bcc402050828c0092d3e85dd37d`. |
| Ratatui/Crossterm removed from `everything` dependency graph | PASS | cleaned manifests + regenerated lockfile in `c1980a7d77144bf8b80ba8bcf8cb8d2b382816a0`. |
| Blocking line-oriented interactive shell | PASS | `crates/aer-cli/src/shell.rs`. |
| No eager environment/runtime/spec discovery at interactive startup | PASS | `commands::run_cli` + `shell::run` call graph. |
| `status` avoids environment fingerprint | PASS | `commands::print_status`; environment discovery appears only in `print_doctor`. |
| Only implemented capability commands advertised | PASS | shell help contract test. |
| Removed UI-only commands cannot leak into help | PASS | `help_does_not_advertise_removed_ui_only_surfaces`. |
| `--workspace PATH` does not trigger eager discovery | PASS | command tests. |
| Research import preserves path spaces and workspace-relative resolution | PASS | shell parser test. |
| Workspace-wide `-D warnings` Clippy | PASS | finalizer `31916419771`; repository CI `31916597879`. |
| Full workspace tests after dependency prune | PASS | finalizer `31916419771`; repository CI `31916597879`. |
| Normal Linux CI including zero-redraw guard | PASS | `foundation-ci` `31916597879`. |
| Canonical isolated Windows CI verifier | PASS | `foundation-ci` `31916597879`. |
| Target Windows verifier + real shell launch | PENDING | required before Step 08. |

## Step 07 exit condition

Repository-side zero-redraw CLI gates are satisfied. Do **not** start Step 08 until the target Windows checkout reproduces the rebuilt CLI successfully.

Canonical target-Windows gate:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
& ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
```

The expected interactive shape is a normal terminal prompt, not a full-screen application. Verify at minimum:

```text
/help
/workspace
/status
/intent
/ir
/runs
/doctor
/quit
```

A final `everything Windows verification: PASS` plus a responsive real interactive shell launch closes the product-side Step-07 gate and makes Step 08 READY.
