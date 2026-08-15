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

rustup run 1.85.0 cargo fmt --all --check
rustup run 1.85.0 cargo clippy --workspace --all-targets -- -D warnings
rustup run 1.85.0 cargo test --workspace --all-targets
rustup run 1.85.0 cargo run -p aer-doc-check -- --check
```

The verification commands intentionally invoke Cargo through `rustup run`. This remains reliable
when another `cargo.exe` appears earlier on Windows `PATH` and therefore does not support
rustup's `cargo +toolchain` shorthand.

All four commands must pass before Step 1 is marked Windows-verified.

## Architecture authority

Implementation must follow the precedence and change discipline in
`docs/00_READ_ME_FIRST.md`. Architecture changes require an ADR; implementation details that
preserve accepted contracts do not.
