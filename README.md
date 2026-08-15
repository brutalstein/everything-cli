# AER — Adaptive Engineering Runtime

This repository is the implementation workspace for the architecture defined in `docs/`.
AER is a local-first, model-agnostic software-engineering runtime whose acceptance unit is
verified engineering outcome rather than model activity.

## Current implementation state

Implementation has started at **Phase 0 / Step 1: Foundation Bootstrap**.
See [`STATUS.md`](STATUS.md) for the authoritative development checkpoint and
[`DEVELOPMENT_PLAN.md`](DEVELOPMENT_PLAN.md) for the execution sequence.

The current code intentionally contains only:

- a dependency-free `aer-domain` crate for stable contract identities, compatibility surfaces,
  and bounded-resource primitives;
- a dependency-free `aer-doc-check` tool that verifies the architecture/ADR/schema/example
  inventory and `docs/MANIFEST.sha256` coverage;
- Linux + Windows CI for format, lint, tests, and documentation integrity.

It does **not** yet implement the daemon, model providers, storage, sandboxing, or the product CLI.
Those are introduced only when their roadmap prerequisites are executable.

## Windows development bootstrap

Prerequisites:

- Git
- `rustup` / Rust toolchain manager

PowerShell:

```powershell
git clone https://github.com/brutalstein/everything-cli.git
cd everything-cli

rustup toolchain install 1.85.0 --profile minimal --component rustfmt --component clippy

cargo +1.85.0 fmt --all --check
cargo +1.85.0 clippy --workspace --all-targets -- -D warnings
cargo +1.85.0 test --workspace --all-targets
cargo +1.85.0 run -p aer-doc-check -- --check
```

All four commands must pass before Step 1 is marked Windows-verified.

## Architecture authority

Implementation must follow the precedence and change discipline in
`docs/00_READ_ME_FIRST.md`. Architecture changes require an ADR; implementation details that
preserve accepted contracts do not.
