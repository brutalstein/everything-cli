# everything

**One CLI for work that spans everything.**

`everything` is a local-first, model-agnostic engineering runtime with a keyboard-first terminal
product surface. The architecture is defined in `docs/`; implementation progress is tracked in
[`STATUS.md`](STATUS.md) and the model-sized execution sequence lives in
[`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md).

The public product name and executable are **`everything`**. `AER` may still appear inside the
architecture as the internal runtime/system terminology, but it is not the primary end-user brand.

## What exists today

The repository currently contains the executable foundations for:

- checked-in Draft 2020-12 contracts with structural and deterministic semantic validation;
- crash-safe local durable state with SQLite WAL/FULL durability, immutable event history,
  project-scoped SHA-256 objects, migration checks, and deterministic replay;
- deterministic runtime-safety primitives: project/run/task state machines, leases, heartbeat and
  reconciliation, bounded queues, cancellation, resource hard caps, and verifier capacity reserve;
- `aer-workspace`, which treats the user's working tree as evidence rather than a worker sandbox,
  captures bounded dirty state, and can reproduce an exact snapshot in an owned Git worktree;
- `aer-exec`, a typed direct-process Tool ABI boundary with explicit side-effect classes, bounded
  output capture, timeout handling, cwd enforcement, and fail-closed strong-isolation semantics;
- `aer-environment`, which fingerprints OS/toolchain/lockfile/environment evidence without storing
  raw environment secrets;
- the first real **`everything` terminal application**, built with a keyboard-first TUI plus
  scriptable/headless subcommands.

Provider execution and authentication, the complete single-agent application runtime, and strong
sandbox backends intentionally remain later roadmap work. The Providers screen therefore does not
pretend that an account is connected before the provider gateway actually exists.

## Run everything on Windows

Prerequisites:

- Git
- `rustup`
- Microsoft Visual Studio Build Tools / Visual Studio with the native x64 C++ toolchain

For the existing checkout used during development:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
```

The verifier pins `1.97.1-x86_64-pc-windows-msvc`, neutralizes conflicting local Rust/Cargo/linker
overrides, runs the complete locked verification suite, and explicitly builds the product binary.
A successful run ends with:

```text
everything Windows verification: PASS
```

It also prints the exact product path. With the default repository layout, launch the interactive
terminal application with:

```powershell
.\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe
```

No arguments + an interactive terminal opens the TUI. Core navigation is deliberately discoverable:

```text
↑ ↓ ← →     navigate
Enter       open / confirm
Esc         back / close
Tab         switch focus
Ctrl+K      command palette
Ctrl+P      providers
Ctrl+L      activity
Ctrl+,      settings
?           contextual help
q           quit when no text-input overlay owns the key
```

The command palette is searchable from the keyboard. The UI adapts between a wide split-pane layout
and a narrower compact layout instead of assuming one terminal size.

## Headless / scriptable use

Interactive UX does not replace CLI automation. The same binary supports non-interactive surfaces:

```powershell
$everything = ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"

& $everything status
& $everything status --json
& $everything doctor
& $everything doctor --json
& $everything workspace
& $everything workspace --json
& $everything providers
```

`status`, `doctor`, and `workspace` are backed by the same workspace/environment boundaries used by
the interactive product. When stdout/stdin are not terminals, invoking `everything` without a
subcommand falls back to plain status output instead of forcing an alternate-screen TUI into a pipe
or CI job.

## Windows verification contract

`scripts/verify-windows.ps1` is the canonical Windows gate. It:

- installs/uses exact `1.97.1-x86_64-pc-windows-msvc`;
- pins the exact Cargo, rustc, and rustdoc executables from that toolchain;
- compiles explicitly for `x86_64-pc-windows-msvc`;
- uses `target/verify-windows-msvc` so incompatible local artifacts cannot leak in;
- temporarily removes process-level Rust/Cargo/native-linker overrides and restores them afterward;
- runs format, locked Clippy with warnings denied, locked workspace tests, workspace/execution/TUI
  tests through the workspace, durable-state regression tests, documentation integrity, and the
  Phase-0 executable-contract regression gate;
- explicitly builds `everything.exe` and fails if the expected binary was not produced.

This prevents a machine with LLVM/MinGW/custom Rust installations from silently changing the
compiler, target, wrapper, linker, or target artifact set used as verification evidence.

## Provider onboarding contract

Provider authentication is intentionally a product workflow, not hidden adapter configuration.
Where a provider officially supports third-party CLI OAuth, `everything` will prefer the provider's
official OAuth 2.0 + PKCE/device authorization path; otherwise it will use the provider-supported API
key/token mechanism. It must never fake OAuth through browser cookies or undocumented consumer
endpoints. Persistent raw credentials stay outside SQLite, events, objects, logs, telemetry, and
prompts and will be stored through an OS secure credential-store adapter.

## Architecture authority

Implementation follows the precedence and change discipline in `docs/00_READ_ME_FIRST.md`.
Architecture changes require an ADR; implementation details that preserve accepted contracts do not.
