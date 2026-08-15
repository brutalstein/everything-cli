# everything Implementation Status

**Last updated:** 2026-08-16  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 1 — Durable, Safe Single-Agent Runtime  
**Current step:** 06 / 18 — Single-Agent Runtime 0.1  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-06 HEAD:** `6b7f1067c2fff104aa68f021a4c713d5cbc273d8`  
**Verified Step-06 CI:** `foundation-ci` run `31911224304` — Ubuntu PASS, canonical isolated Windows verifier PASS  
**Next step:** 07 — Intent + Research + Engineering IR — BLOCKED until Step-06 target Windows verification passes

## Completed milestones

- **Step 01 — Foundation Bootstrap:** COMPLETE — CI `31899011790`.
- **Step 02 — Executable Contract System:** COMPLETE — CI `31903313314`; target Windows PASS.
- **Phase 0:** COMPLETE.
- **Step 03 — Durable State Kernel:** COMPLETE — canonical hardened CI `31905250522`; target Windows PASS.
- **Step 04 — Runtime State + Resource Safety:** COMPLETE — CI `31906368065`; target Windows PASS.
- **Step 05 — Workspace + Execution Boundary:** COMPLETE — CI `31909059844`; target Windows verifier and real interactive `everything.exe` launch confirmed by the user on 2026-08-16.

## Product-surface development rule

Backbone first, TUI in parallel. Core/domain/runtime/storage/execution design is never distorted for interface progress. Once a capability is stable, truthful, and meaningfully usable, it is projected through the same `everything` application APIs into the TUI/headless CLI in the same implementation slice or immediately adjacent integration commit. The TUI may not duplicate business logic, invent authority, or fabricate unavailable state.

## Premium `everything` terminal surface

The terminal product is now a modular Ratatui/Crossterm presentation layer rather than the original minimal shell.

Implemented and repository-verified:

- prominent `everything` hero/wordmark and tagline;
- near-black premium shell with cyan primary and violet secondary accents;
- centralized Material-like terminal glyph set with `EVERYTHING_ASCII=1` fallback;
- `NO_COLOR` support and truecolor use only when the terminal advertises it;
- wide premium Home composition with Workspace, Command Menu, Connected Surfaces, and Next Recommended Action cards;
- compact/narrow application shell for smaller terminals;
- Home, Workspace, Environment, Providers, Activity, and Settings surfaces;
- arrows, Enter, Esc, Tab/Shift+Tab, `Ctrl+K`, `Ctrl+P`, `Ctrl+L`, `Ctrl+,`, `?`, and safe `q` behavior;
- searchable command palette and contextual help;
- deterministic render/navigation tests across wide and narrow terminal sizes;
- premium base independently verified on Ubuntu + canonical Windows in CI run `31910231344`.

The terminal does not require a specific icon font. Unicode glyphs are centralized and degrade to ASCII rather than turning missing Material/Nerd fonts into broken boxes.

## Step 06 — Single-Agent Runtime 0.1

**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING

### Provider-neutral gateway

New `aer-provider` crate provides:

- normalized `ProviderRequest` / `ProviderResponse` contracts;
- provider descriptor and declared authentication methods;
- normalized failure classes: authentication, invalid request, rate limited, transient, cancelled, permanent;
- only rate-limit/transient failures are retryable;
- hard bounded attempt count and clamped exponential/provider retry delay;
- cancellation before attempts and during retry backoff;
- deterministic `ReferenceProvider` for CI/E2E only.

The reference provider is explicitly `production_ready=false` and `AuthenticationMethod::TestOnly`. It is never presented in the product as a connected user account.

### Single-agent application/runtime API

New `aer-core` crate provides the shared in-process application API consumed by product surfaces. Phase 1 does not add a speculative always-running daemon where a second-process client is not yet required.

A run now performs a real bounded vertical slice:

1. inspect/capture exact user workspace state;
2. derive stable project identity;
3. create runtime state outside the user repository;
4. materialize an owned detached worktree;
5. append durable `run.created` and state events;
6. call the provider-neutral gateway;
7. structurally validate a bounded JSON edit plan;
8. persist the provider response as a project-scoped CAS artifact;
9. support deliberate interruption after provider response or applied edits;
10. reconstruct the run from durable events and CAS on resume;
11. enter explicit `Recovering`, then transition back to the safe execution/verification target;
12. apply edits only inside the owned worktree;
13. execute a trusted verifier command through the existing explicit-argv process boundary;
14. bind command evidence to exact repository/environment identity;
15. require both verifier success and independently supplied expected-file hashes;
16. publish minimal operational acceptance and terminal run state;
17. verify project event/object integrity.

### Runtime safety / injection boundaries

- provider output never receives shell-command authority;
- the provider may propose only bounded file-content replacements;
- verifier executable/argv come from trusted application/user acceptance input, not model output;
- maximum plan/edit counts and byte budgets are enforced;
- absolute, parent, empty-component, backslash, colon/NTFS-ADS-like, NUL/control-character paths fail closed;
- `.git` and `.aer` path components are rejected case-insensitively even inside the owned worktree;
- target symlinks are rejected;
- canonical parent paths must remain inside the owned worktree;
- verification has a fixed timeout and bounded captured output;
- user working-tree identity is compared before/after in E2E tests and must remain unchanged.

Full ProofManifest composition, held-out verifier immutability, anti-reward-hacking verification, and domain-profile proof policy remain Step 10; Step 06 intentionally implements only the minimum trusted acceptance path needed for the first resumable single-agent runtime.

### Durable interruption / resume evidence

The principal E2E test creates a real Git repository with tracked and untracked user state, starts a run, persists the provider plan, intentionally interrupts, then resumes with a fresh runtime instance whose provider output must not be called again. The run reconstructs from durable events/CAS, edits only the owned worktree, verifies, accepts, completes, and leaves the original user workspace byte/state identity unchanged.

A second adversarial test rejects provider plans attempting to escape or target control-plane/non-portable paths.

### TUI/headless integration

Step-06 runtime capabilities were integrated immediately into the product surface rather than waiting for a later UI project:

- `AppState` reads the real durable run catalog from `aer-core`;
- Home projects runtime health/run count;
- Connected Surfaces includes the real runtime state;
- Activity shows durable runs, states, goals, accepted/interrupted state, or an explicit runtime read error;
- Next Recommended Action prioritizes an existing resumable run, otherwise provider setup;
- Providers truthfully says gateway ready but production profile not configured;
- runtime/catalog errors are never converted into fake zero-run success;
- headless `everything runs [--json]` is available;
- `everything status` and `everything doctor` now include runtime health/catalog information.

A production provider profile is deliberately not fabricated. The previously recorded authentication rule remains: use official OAuth 2.0 + PKCE/device authorization only where the provider officially supports third-party CLI OAuth; otherwise use the provider-supported API-key/token flow, and never persist raw credentials in normal durable state/logs/prompts/telemetry.

## Step 06 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Provider failures are normalized | PASS | `aer-provider` tests. |
| Retry count/backoff is bounded | PASS | retry-policy + transient-retry tests. |
| Authentication failure is not blindly retried | PASS | provider failure-class test. |
| Provider request cancellation is honored | PASS | cancellation test. |
| Provider output cannot directly execute shell commands | PASS | edit-plan-only runtime contract. |
| Provider edits cannot escape owned worktree | PASS | adversarial E2E test. |
| `.git` / `.aer` and non-portable path forms fail closed | PASS | hardened edit-path tests. |
| Start can be interrupted after durable provider response | PASS | single-agent E2E. |
| Resume reconstructs from durable events/CAS | PASS | fresh-runtime E2E. |
| Resume explicitly passes through `Recovering` | PASS | runtime state transition implementation exercised by E2E. |
| Persisted provider response prevents unnecessary re-call on resume | PASS | E2E resumes with an unusable replacement provider. |
| User working tree remains unchanged | PASS | before/after `WorkspaceIdentity` equality in E2E. |
| Verification evidence binds repo + environment | PASS | `CommandExecutionEvidence` path. |
| Runtime catalog is projected into TUI/headless CLI | PASS | `everything` terminal tests + integration. |
| Linux format + `-D warnings` Clippy + full workspace tests | PASS | `31911224304`. |
| Dedicated Single-Agent Runtime 0.1 gate | PASS | `31911224304`. |
| Dedicated terminal product surface gate | PASS | `31911224304`. |
| Workspace/storage/docs/contracts regressions | PASS | `31911224304`. |
| Canonical isolated Windows CI verifier | PASS | `31911224304`. |
| Target Windows canonical verification + interactive launch | PENDING | Pull `main`, run verifier, launch `everything.exe`. |

## Step 06 exit condition

Repository-side Step-06 gates are satisfied. Do **not** start Step 07 until the target Windows checkout reproduces the current canonical verifier and the updated terminal product launches successfully:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
& ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
```

Useful headless checks:

```powershell
$everything = ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
& $everything status
& $everything runs
& $everything doctor
& $everything providers
```

A final `everything Windows verification: PASS` plus successful interactive launch closes Step 06 and makes Step 07 READY.
