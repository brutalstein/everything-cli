# everything

**One CLI for work that spans everything.**

`everything` is a local-first, model-agnostic engineering runtime. The architecture is defined in
`docs/`; implementation progress is tracked in [`STATUS.md`](STATUS.md), and the model-sized build
sequence lives in [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md).

The public executable is **`everything`**. `AER` remains internal architecture terminology where the
architecture uses it.

## What exists today

The implemented system currently includes:

- checked-in Draft 2020-12 contracts with deterministic structural and semantic validation;
- crash-safe local durable state with SQLite WAL/FULL durability, immutable event history,
  project-scoped SHA-256 objects, migration checks, and deterministic replay;
- deterministic runtime-safety primitives: state machines, leases, heartbeat/reconciliation,
  bounded queues, cancellation and hard resource admission limits;
- `aer-workspace`, which captures authoritative repository identity and bounded dirty-state evidence
  and can reproduce an owned Git worktree;
- `aer-exec`, a typed direct-process Tool ABI with explicit side-effect classes, bounded output,
  timeout handling and fail-closed isolation semantics;
- `aer-environment`, which fingerprints OS/toolchain/lockfile evidence without storing raw secrets;
- the single-agent runtime/application boundary and provider gateway foundation;
- durable intent, explicit user semantics/decisions, source-backed ResearchArtifact ingestion,
  Engineering IR compilation, semantic checksums and monotonic SpecDelta revisions;
- a scriptable headless CLI plus a deliberately minimal interactive line shell.

Production provider authentication/onboarding is not implemented yet. The CLI reports that state
plainly instead of presenting a fake connected account or settings workflow.

## Interactive CLI

There is **no full-screen TUI**.

A bare interactive launch starts a small line-oriented shell:

```text
everything
workspace C:\path\to\repo
type /help for available commands

❯
```

The shell is intentionally simple for latency and correctness:

- no alternate terminal screen;
- no Ratatui/Crossterm renderer;
- no per-key redraw/event loop;
- no theme/icon/layout engine;
- no eager environment fingerprint, runtime catalog, or specification replay on startup;
- blocking line input while idle;
- architecture work is performed only when the corresponding command is requested.

Use another repository without changing directory:

```powershell
& $everything --workspace C:\path\to\repo
```

Available interactive commands are limited to capabilities that exist in the current implementation:

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

Ordinary text is recorded through the authoritative `SpecService::submit_message` boundary. The
explicit semantic commands call the existing user-authoritative specification APIs. `/runs` reads the
durable runtime ledger. `/research-import` accepts an already acquired ResearchArtifact and does not
fabricate network research. `/doctor` is intentionally the heavier diagnostic and is the only normal
interactive command that performs the complete environment fingerprint.

## Headless / scriptable use

The same binary supports non-interactive inspection:

```powershell
$everything = ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"

& $everything status
& $everything status --json
& $everything workspace
& $everything workspace --json
& $everything intent
& $everything intent --json
& $everything ir
& $everything ir --json
& $everything research
& $everything research --json
& $everything runs
& $everything runs --json
& $everything providers
& $everything doctor
& $everything doctor --json
```

Every subcommand also accepts the global workspace selector:

```powershell
& $everything --workspace C:\path\to\repo status
```

When stdin/stdout are not terminals, invoking `everything` without a subcommand prints plain status
instead of attempting an interactive shell.

## Run and verify on Windows

Prerequisites:

- Git
- `rustup`
- Microsoft Visual Studio Build Tools / Visual Studio with the native x64 C++ toolchain

Canonical verification:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
```

The verifier pins `1.97.1-x86_64-pc-windows-msvc`, neutralizes conflicting Rust/Cargo/linker
overrides, runs locked format/lint/test/contract gates, and explicitly builds the product binary. A
successful run ends with:

```text
everything Windows verification: PASS
```

Launch the verified binary:

```powershell
& ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
```

## Performance contract for the CLI surface

The interactive shell must remain a projection over application boundaries, not a continuously
rendered application state cache.

Repository CI therefore guards the current design:

- the `everything` dependency tree may not contain `ratatui` or `crossterm`;
- retired full-screen TUI source/assets may not reappear;
- workspace-wide Clippy runs with warnings denied;
- full workspace tests and the dedicated `everything` package tests must pass;
- the canonical isolated Windows verification must build the same product.

Performance-sensitive diagnostics are lazy by construction. `status` does not perform an environment
fingerprint; `doctor` does. Interactive startup does not call `WorkspaceIdentity`, `SpecService`,
`list_runs`, or `EnvironmentFingerprint` before the user requests work.

## Architecture authority

Implementation follows the precedence and change discipline in `docs/00_READ_ME_FIRST.md`.
Architecture changes require an ADR; product-surface implementation changes that preserve accepted
contracts do not.
