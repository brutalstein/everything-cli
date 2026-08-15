# AER — Adaptive Engineering Runtime

This repository is the implementation workspace for the architecture defined in `docs/`.
AER is a local-first, model-agnostic software-engineering runtime whose acceptance unit is
verified engineering outcome rather than model activity.

## Current implementation state

**Phase 1 / Step 03: Durable State Kernel** is under repository verification.
See [`STATUS.md`](STATUS.md) for the authoritative development checkpoint and
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

PowerShell:

```powershell
git clone https://github.com/brutalstein/everything-cli.git
cd everything-cli

rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy

rustup run 1.97.1 cargo fmt --all --check
rustup run 1.97.1 cargo clippy --locked --workspace --all-targets -- -D warnings
rustup run 1.97.1 cargo test --locked --workspace --all-targets
rustup run 1.97.1 cargo test --locked -p aer-storage --all-targets
rustup run 1.97.1 cargo run --locked -p aer-doc-check -- --check
rustup run 1.97.1 cargo run --locked -p aer-phase0-check -- --check
```

The verification commands intentionally invoke Cargo through `rustup run`. This remains reliable
when another `cargo.exe` appears earlier on Windows `PATH` and therefore does not support
rustup's `cargo +toolchain` shorthand. Dependency-consuming commands also use `--locked`; a stale
or missing lockfile is a verification failure rather than permission to silently re-resolve.

## Architecture authority

Implementation must follow the precedence and change discipline in
`docs/00_READ_ME_FIRST.md`. Architecture changes require an ADR; implementation details that
preserve accepted contracts do not.
