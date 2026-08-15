# everything Implementation Status

**Last updated:** 2026-08-15  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 1 — Durable, Safe Single-Agent Runtime  
**Current step:** 05 / 18 — Workspace + Execution Boundary  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction and interactive TUI launch  
**Verified Step-05 HEAD:** `3ffcddc842ec219df63d7440f48efd8da30514cf`  
**Verified Step-05 CI:** `foundation-ci` run `31909059844` — Ubuntu PASS, canonical isolated Windows verifier PASS  
**Next step:** 06 — Single-Agent Runtime 0.1 — BLOCKED until Step-05 target Windows verification passes

## Completed milestones

- **Step 01 — Foundation Bootstrap:** COMPLETE  
  Implementation `6495c77dbc05d7db635062a35bb3bc0eb0857922`; CI `31899011790`.
- **Step 02 — Executable Contract System:** COMPLETE  
  Implementation `6f9c4258299a5f9880cdec78c976aaa56bfb884d`; CI `31903313314`; target Windows PASS.
- **Phase 0:** COMPLETE.
- **Step 03 — Durable State Kernel:** COMPLETE  
  Implementation `c8f1f6153cc076a6e4c1b93e8c8d6da903a80fa5`; canonical Windows verifier hardening `0a16edfda161bdf8d4d9e2b51068a393462671fa`; target Windows PASS.
- **Step 04 — Runtime State + Resource Safety:** COMPLETE  
  Repository CI `31906368065`; target Windows canonical verifier PASS on 2026-08-15. User-supplied output showed 26/26 `aer-domain` tests, 15/15 `aer-storage` tests, documentation integrity PASS, Phase-0 executable contracts PASS, and final `AER Windows verification: PASS`.

## Step 05 — Workspace + Execution Boundary

**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING

### Verified implementation

- initial workspace/execution/environment boundary: `eb839c60d8917c60a9121ad52f779076716de511`;
- public `everything` terminal product surface: `efb983155420d3d3fefaee2d1bfaecf020ab61e0`;
- TUI dependency/format lock state: `e5edd2e80bb46b8c6df8132dfeb69b80fe8debd6`;
- TUI compiler integration repair: `fccef4918b29b41fa1c62568d1376408372b31e2`;
- Windows verifier now builds and checks the real `everything.exe` product;
- final verified Step-05 HEAD: `3ffcddc842ec219df63d7440f48efd8da30514cf`;
- final read-only repository CI: `foundation-ci` run `31909059844`.

### Workspace boundary

- canonical repository identity includes root, HEAD, optional branch, sanitized remotes, tracked-dirty patch identity, untracked inventory identity, and submodule-state identity;
- remote URL credentials/query/fragment data are removed before becoming workspace identity evidence;
- the user's working tree is treated as evidence and is never used as a worker sandbox;
- tracked dirty state is captured as bounded binary Git patch data;
- untracked state is inventoried with NUL-delimited Git output and captured under explicit per-file and total byte bounds;
- non-regular untracked entries fail closed instead of being silently copied;
- before/after state checks reject snapshots when the workspace changes during capture;
- excluding relevant untracked state marks the snapshot inexact and prevents exact worktree materialization;
- exact snapshots materialize into detached owned Git worktrees at the same HEAD;
- tracked patch and untracked files are reproduced without branch switching, reset, stash, clean, or mutation of the user working tree;
- materialized worktree identity is re-inspected and must match the captured tracked/untracked identities;
- failed worktree materialization cleans both the Git worktree registration and destination directory;
- paths containing spaces are covered by integration tests.

### Workspace mutation ownership / Windows locking

- one logical repository has one non-blocking mutating coordinator lock;
- lock files live in a caller-owned runtime directory rather than the user's repository;
- the lock filename is derived from SHA-256(repo identity), so it is portable and Windows-safe;
- file existence is not authority: the live OS file lock handle is;
- stale lock files after process crash do not permanently lock a repository;
- same-repository contention fails fast with an explicit `AlreadyLocked` result;
- different repository identities do not contend;
- explicit release and ordinary Drop both explicitly unlock before the file handle closes;
- lock metadata is not truncated until after ownership is acquired, preventing a losing coordinator from rewriting the active owner's diagnostics;
- Linux and Windows tests both verify contention/release behavior.

### Typed command / process boundary

- `aer-exec` executes explicit argv without invoking a shell parser;
- canonical cwd must remain inside the declared workspace boundary;
- process environment starts cleared and inherits only an explicit small baseline plus command-specific values;
- output preview is bounded while complete stdout/stderr identities remain SHA-256 hashed;
- timeout performs kill + wait/reap semantics rather than abandoning child processes;
- side effects are typed (`PureRead`, `WorkspaceWrite`, `ProcessExecution`, network/external/credential/privileged classes);
- direct-host execution supports only the intentionally allowed low-authority classes;
- high-authority effects fail closed even if a caller asks to allow them on this adapter;
- `SecurityProfile::DirectHostProcess` is explicit: this adapter is not represented as a strong sandbox;
- requiring strong isolation on the direct-host adapter fails closed instead of silently downgrading.

### Environment and command evidence

- environment fingerprint binds OS, architecture, family, best-effort OS version, shell/locale/timezone metadata, selected tool versions, root lockfile hashes, and selected non-secret environment-signal hashes;
- raw environment values are not stored as evidence;
- lockfile content changes change the environment digest;
- OS capability reporting is conservative and distinguishes Unix/Windows process semantics, native file locking, and the fact that strong process isolation is not implemented yet;
- `CommandExecutionEvidence` binds command result identity to both `repo_id` and `environment_digest`;
- evidence includes argv, cwd, exit/success/timeout state, output hashes/byte counts, duration, and the truthful security profile;
- changing repository identity or environment digest changes the command evidence digest.

### `everything` terminal product surface

The repository now contains a real executable product package named **`everything`**, not a mock screenshot.

Interactive mode is keyboard-first:

- arrows navigate;
- `Enter` opens/confirms;
- `Esc` goes back/closes overlays;
- `Tab` / `Shift+Tab` move deterministic focus;
- `Ctrl+K` opens a searchable command palette;
- `Ctrl+P` opens Providers;
- `Ctrl+L` opens Activity;
- `Ctrl+,` opens Settings;
- `?` opens contextual keyboard help;
- `q` quits only when a text/palette input surface does not own the character.

Current real screens:

- Home — projects real workspace/environment state;
- Workspace — repository identity and dirty-state evidence;
- Environment — OS/toolchain/lockfile fingerprint information;
- Providers — truthfully reports that no provider is configured yet rather than fabricating connectivity;
- Activity — truthfully reports no active run until Step 06 exists;
- Settings — exposes the keyboard interaction model without creating a parallel configuration store.

The layout has separate wide and compact render paths. Tests exercise 100x30 and 52x20 terminal sizes and deterministic navigation/palette behavior. Terminal resize events are accepted without owning runtime semantics.

Headless/scriptable surfaces remain available from the same binary:

```text
everything status [--json]
everything doctor [--json]
everything workspace [--json]
everything providers
```

If stdin/stdout are not terminals, invoking `everything` without a subcommand falls back to plain status instead of forcing an alternate-screen TUI into CI or a pipe.

### Step 05 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Dirty user working tree remains unchanged | PASS | Dirty tracked + untracked worktree materialization tests on Linux/Windows CI. |
| Exact dirty state materializes in owned worktree | PASS | `aer-workspace` integration tests. |
| Inexact snapshot cannot be silently materialized | PASS | Explicit fail-closed test. |
| Worktree failure cleans registration + directory | PASS | `crates/aer-workspace/tests/recovery.rs`. |
| Remote credentials excluded from workspace identity | PASS | Sanitization test. |
| Single mutating coordinator per logical repo | PASS | OS file-lock contention tests on Linux and Windows. |
| Drop/release permits reacquisition | PASS | Explicit release and scope-drop tests on Linux and Windows. |
| Direct host adapter cannot masquerade as strong sandbox | PASS | `StrongIsolationUnavailable` test. |
| Cwd escape is rejected before child spawn | PASS | `aer-exec` test. |
| High-authority direct-host side effects fail closed | PASS | `aer-exec` authority test. |
| Command evidence binds repo + environment identity | PASS | `aer-environment::evidence` tests. |
| OS capability baseline is explicit/conservative | PASS | `aer-environment::capabilities` test. |
| Keyboard navigation / palette / input ownership | PASS | `everything` TUI tests. |
| Wide and narrow terminal rendering | PASS | Ratatui `TestBackend` tests. |
| Linux format + Clippy + complete workspace suite | PASS | `foundation-ci` run `31909059844`. |
| Dedicated Workspace + execution boundary gate | PASS | `foundation-ci` run `31909059844`. |
| Dedicated Terminal product surface gate | PASS | `foundation-ci` run `31909059844`. |
| Durable-state + contract regressions | PASS | `foundation-ci` run `31909059844`. |
| Canonical isolated Windows CI verifier | PASS | `foundation-ci` run `31909059844`. |
| Real `everything.exe` production inside verifier | PASS | Windows verifier would fail if the expected binary were absent; run `31909059844` completed successfully. |
| Target Windows canonical verification + interactive launch | PENDING | Pull `main`, run `.\scripts\verify-windows.ps1`, then launch the printed `everything.exe`. |

## Provider authentication/onboarding requirement for Step 06

Provider authentication remains a first-run/settings product workflow. Use official OAuth 2.0 + PKCE/device authorization only where a provider officially supports third-party CLI OAuth; otherwise use the provider-supported API-key/token mechanism. Never emulate OAuth with cookies or undocumented consumer endpoints. Raw credentials stay out of SQLite, events, objects, logs, telemetry, and prompts; persistent secrets belong in an OS secure credential-store adapter while durable state stores only opaque references and non-secret profile metadata.

The Step-05 Providers screen intentionally shows `not configured` until this real gateway exists.

## Step 05 exit condition

Repository-side Step-05 gates are satisfied. Do **not** start Step 06 until the target Windows checkout reproduces the canonical verifier and the actual terminal product launches successfully:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
.\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe
```

A final `everything Windows verification: PASS` plus a successful interactive launch closes Step 05 and makes Step 06 READY.
