# AER — Adaptive Engineering Runtime

This repository is the implementation workspace for the architecture defined in `docs/`.
AER is a local-first, model-agnostic software-engineering runtime whose acceptance unit is
verified engineering outcome rather than model activity.

## Current implementation state

**Phase 1 / Step 03: Durable State Kernel** is repository-verified and awaiting target Windows
reproduction. See [`STATUS.md`](STATUS.md) for the authoritative development checkpoint and
[`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) for the execution sequence.

The current foundation contains:

- a dependency-free `aer-domain` crate for stable contract identities, compatibility surfaces,
  and bounded-resource primitives;
- `aer-contracts`, which compiles the 15 checked-in Draft 2020-12 schemas from a local registry,
  performs structural/compatibility validation, and enforces deterministic cross-object rules;
- `aer-storage`, the initial crash-safe local durable-state kernel with SQLite WAL/FULL durability,
  an immutable append-only event journal, project-scoped SHA-256 content-addressed objects,
  migration/preflight checks, and deterministic replayable journal projections;
- a benchmark fixture ABI and minimal OpenTelemetry API adapter without a provider SDK/exporter;
- `aer-doc-check` for architecture/ADR/schema/example inventory and manifest coverage;
- `aer-phase0-check` for shipped examples, negative boundary fixtures, semantic fixtures,
  compatibility fixtures, and normative configuration examples;
- a checked-in `Cargo.lock` and `--locked` CI gates so dependency resolution is reproducible;
- Linux + Windows CI for format, lint, tests, durable-state conformance, documentation integrity,
  and the Phase-0 contract gate.

It does **not** yet implement project/run/task state machines, model-provider execution, the daemon,
sandboxing, or the product CLI. Those are introduced only when their roadmap prerequisites are
executable.

Provider authentication/onboarding is specified ahead of implementation in
`docs/37_PROVIDER_GATEWAY_AND_RESILIENCE.md`: first-run/provider settings will use official OAuth
flows when a provider supports third-party CLI OAuth and supported API-key/token authentication
otherwise, with secrets kept outside ordinary AER durable state.

## Windows development bootstrap

Prerequisites:

- Git
- `rustup` / Rust toolchain manager
- Microsoft Visual Studio Build Tools / Visual Studio with the native x64 C++ toolchain

PowerShell:

```powershell
git clone https://github.com/brutalstein/everything-cli.git
cd everything-cli

.\scripts\verify-windows.ps1
```

For an existing checkout:

```powershell
cd C:\path\to\everything-cli
git pull origin main
.\scripts\verify-windows.ps1
```

The Windows verifier is intentionally the canonical local gate instead of a copied list of Cargo
commands. It installs/uses the exact `1.97.1-x86_64-pc-windows-msvc` toolchain, compiles explicitly
for `x86_64-pc-windows-msvc`, uses an isolated verification target directory, and temporarily
neutralizes process-level Rust/Cargo/native-build overrides that could otherwise redirect the build
to another compiler, target, wrapper, linker, or C toolchain. The original environment values are
restored before the script returns.

This matters on machines that also contain LLVM/MinGW/custom Rust installations: local verification
must not silently become `x86_64-pc-windows-gnullvm`, inherit an older `RUSTC`, or reuse incompatible
target artifacts. Dependency-consuming commands remain `--locked`; a stale or missing lockfile is a
verification failure rather than permission to silently re-resolve.

The verifier runs the same semantic gates as CI:

```text
format
locked workspace Clippy with warnings denied
locked workspace tests
locked aer-storage tests
documentation integrity
Phase-0 executable-contract regression gate
```

## Architecture authority

Implementation must follow the precedence and change discipline in
`docs/00_READ_ME_FIRST.md`. Architecture changes require an ADR; implementation details that
preserve accepted contracts do not.
