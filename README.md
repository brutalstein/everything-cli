# everything

**One CLI for work that spans everything.**

`everything` is a local-first, model-agnostic engineering runtime. The public executable is **`everything`**; **AER** remains the internal architecture name.

Architecture lives in `docs/`, current implementation truth lives in [`STATUS.md`](STATUS.md), and the model-sized build sequence lives in [`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md).

## Current state

Steps 01–13 of the 18-step architecture sequence are complete. Work is currently inside the non-numbered **Provider Runtime Productization Gate** between Step 13 and Step 14. Step 14 is intentionally blocked until that gate closes.

The merged runtime includes:

- executable Draft 2020-12 contracts and semantic validation;
- crash-safe SQLite WAL state, append-only events, content-addressed objects and deterministic replay;
- runtime state machines, leases, cancellation, bounded queues and hard resource admission;
- exact workspace identity, dirty-state capture and AER-owned isolated Git worktrees;
- typed bounded process execution and environment fingerprinting;
- Intent + Research + Engineering IR;
- Repository Intelligence 2.0 and Context Economy;
- proof-carrying independent verification;
- provider resilience, cost routing and truthful provider telemetry;
- long-horizon engineering state and recovery;
- bounded parallel execution with isolated worktrees and integration verification;
- delegated provider onboarding/smoke surfaces for Codex, Claude Code and Gemini CLI;
- AER-owned permission policy and typed ToolBroker foundations;
- provider context-economics and Claude authority-split acceptance diagnostics.

The repository is not claiming that the provider gate is complete. The latest live Claude acceptance matrix exposed an exact-definition retrieval defect: a task asking for `ArchitectureContextCapsule::compile`'s `version` received a Context Pack that stopped before the actual `version: 3` assignment. Both the current Claude preset and the authority-split candidate therefore failed the same task. That retrieval defect must be fixed and the full matrix rerun before production promotion.

See:

- [`docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md`](docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md)
- [`docs/46_PROVIDER_CONTEXT_ECONOMICS_BENCHMARK.md`](docs/46_PROVIDER_CONTEXT_ECONOMICS_BENCHMARK.md)
- [`docs/47_PROVIDER_AUTHORITY_SPLIT_ACCEPTANCE.md`](docs/47_PROVIDER_AUTHORITY_SPLIT_ACCEPTANCE.md)

## Provider runtime

Provider authentication and transport are deliberately separated from AER authority.

Available product commands include:

```text
everything providers
everything provider status [codex|claude|gemini]
everything provider login <provider>
everything provider login codex --device
everything provider logout <provider>
everything provider smoke <provider> --show-input --prompt "..."
everything provider smoke <provider> --json --prompt "..."
everything provider benchmark <provider> --runs 3 --json
```

Current posture:

- **Claude Code:** delegated authenticated smoke is supported under AER isolation controls and is the current target-machine live-validation provider.
- **Codex:** delegated adapter/login/smoke support is implemented; executable/account availability is local-machine dependent.
- **Gemini CLI:** discovery/login is supported, but delegated smoke currently fails closed because its delegated OAuth/user-state boundary cannot yet be separated from provider-local behavior/configuration state strongly enough.

Vendor-owned login remains vendor-owned. `everything` does not scrape or copy consumer OAuth refresh secrets.

### Authority split under evaluation

A controlled Claude cache-attribution lab showed that merely stabilizing the scratch working directory did not materially improve cache reuse. Replacing the generic Claude Code preset with an AER-owned constitutional system authority, while keeping repository/task evidence in the user/data layer, substantially reduced provider input and cost in the diagnostic probe.

The candidate is **not production-default yet**. A multi-task acceptance matrix first requires correctness and adversarial authority invariants to pass. The current blocker is source retrieval, not a candidate-only authority regression.

## Interactive CLI

There is no full-screen TUI. A bare interactive launch starts the deliberately small line-oriented shell:

```text
everything
workspace C:\path\to\repo
type /help for available commands

❯
```

The shell avoids alternate-screen rendering and eager architecture/provider work. Provider discovery occurs only when provider functionality is requested.

Representative commands include:

```text
/status
/workspace
/intent
/ir
/research
/runs
/providers
/doctor
/permission
/provider ...

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

The interface projects existing application/runtime state; it does not maintain a second UI-specific authority model.

## Headless use

The same binary supports scriptable inspection:

```powershell
$everything = ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"

& $everything status
& $everything status --json
& $everything workspace --json
& $everything intent --json
& $everything ir --json
& $everything research --json
& $everything runs --json
& $everything providers
& $everything provider status claude --json
& $everything doctor --json
```

Use another repository without changing directory:

```powershell
& $everything --workspace C:\path\to\repo status
```

## Run and verify on Windows

Prerequisites:

- Git;
- `rustup`;
- Microsoft Visual Studio Build Tools / Visual Studio with the native x64 C++ toolchain.

Canonical verification:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull --ff-only
.\scripts\verify-windows.ps1 -SkipToolchainInstall
```

The verifier pins the repository's Windows MSVC toolchain, neutralizes conflicting Cargo/Rust/linker overrides, runs the locked correctness gates and builds the product.

Success ends with:

```text
everything Windows verification: PASS
```

Launch the verified binary:

```powershell
& ".\target\verify-windows-msvc\x86_64-pc-windows-msvc\debug\everything.exe"
```

## Provider acceptance on Windows

The Claude authority-split matrix is intentionally separate from deterministic CI because it makes real provider calls.

Inspect retrieval first without provider calls:

```powershell
.\scripts\run-provider-acceptance-windows.ps1 -Runs 2
```

Then, only when retrieval is valid:

```powershell
$out = Join-Path $env:TEMP "aer-provider-acceptance.json"

.\scripts\run-provider-acceptance-windows.ps1 -Runs 2 -Live -Json |
    Tee-Object -FilePath $out
```

The live matrix compares the current Claude preset with the AER-owned authority-split candidate on repository facts, architecture authority and an adversarial repository prompt-injection case. Economic improvement alone cannot promote the candidate.

## What is not claimed yet

`everything` does **not** currently claim:

- Provider Runtime Productization Gate completion;
- production promotion of the Claude authority-split candidate;
- universal exact semantic understanding for every programming language;
- a strong sandbox for unrestricted provider-native agentic process execution;
- that provider prompt-cache ratios are an engineering-quality score;
- that Step 14 has started.

Current blockers and the exact next action order are maintained in [`STATUS.md`](STATUS.md).

## Architecture authority

Implementation follows the precedence and change discipline in `docs/00_READ_ME_FIRST.md`.

High-level rule:

> models are replaceable compute; AER owns context, authority, resource budgets, execution boundaries, evidence and acceptance.

Passing visible tests, generating more tokens, using more agents or obtaining a cheaper provider call is not sufficient by itself. The project optimizes verified engineering outcome per unit cost.
