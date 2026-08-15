# Target Repository Structure

This is a target shape, not a command to scaffold everything immediately.

```text
/
├── Cargo.toml
├── README.md
├── docs/
├── crates/
│   ├── aer-domain/            # pure domain types/state machines/invariants
│   ├── aer-core/              # orchestration application layer
│   ├── aer-daemon/            # local app server/runtime
│   ├── aer-cli/               # CLI/TUI client
│   ├── aer-protocol/          # runtime RPC schemas/types + compatibility negotiation
│   ├── aer-store/             # SQLite journal/projections/object metadata/migrations
│   ├── aer-models/            # capability registry + provider gateway/adapters
│   ├── aer-context/           # context pack/ranking/budget
│   ├── aer-repo/              # repository intelligence/indexing/workspace identity
│   ├── aer-research/          # external research acquisition + claim provenance
│   ├── aer-exec/              # process/tool ABI
│   ├── aer-sandbox/           # sandbox backend abstraction
│   ├── aer-resources/         # admission, quotas, leases, backpressure
│   ├── aer-environment/       # environment/dependency fingerprints + supply-chain hooks
│   ├── aer-verify/            # verifier controller/evidence/domain profiles
│   ├── aer-health/            # architecture health adapters
│   ├── aer-telemetry/         # OTel + accounting
│   ├── aer-update/            # compatibility/release/update verification (when Phase 10 arrives)
│   └── aer-integrations/      # MCP/A2A and external boundaries later
├── sdk/
│   ├── typescript/            # later
│   └── python/                # later
├── evals/
│   ├── fixtures/
│   ├── intent/
│   ├── research/
│   ├── context/
│   ├── providers/
│   ├── routing/
│   ├── handoff/
│   ├── resources/
│   ├── verification/
│   ├── evolution/
│   ├── migration/
│   └── security/
└── scripts/
```

## Dependency direction

Conceptually:

```text
aer-domain
   ↑
aer-core
   ↑
ports/traits
   ↑
application adapters
   ↑
daemon / CLI
```

`aer-domain` MUST NOT depend on provider SDKs, database drivers, sandbox/TUI libraries, update clients, or protocol-specific network code.

## Domain package

Owns:

- stable IDs;
- Engineering IR/task/run/evidence semantics;
- facts/hypotheses/decisions;
- policy decisions/value objects;
- security/capability/resource types;
- contract versions and semantic invariants.

Keep it deterministic.

## Core orchestration

`aer-core` coordinates ports for:

- repository/context;
- research;
- providers/models;
- resource admission/scheduler;
- execution/sandbox;
- verification;
- storage;
- telemetry.

It should depend on traits/interfaces, not concrete SDK/backend behavior.

## Adapter packages

Provider-, OS-, package-manager-, protocol-, and sandbox-specific implementations remain shallow.

A new model vendor, research source, package manager, or sandbox backend should not require changes to task/evidence/Engineering IR semantics.

## Crate restraint

The target tree describes conceptual ownership. Do NOT create a crate merely because it appears above.

During early phases, adjacent low-pressure components MAY live as modules in fewer crates. Split only when ownership, dependencies, compile boundaries or independent testing justify it.

## Feature flags

Use feature flags sparingly for optional heavy integrations. Do not create combinatorial build matrices.

## Generated schema/protocol code

Generated bindings belong in clearly marked generated directories and are regenerated through deterministic pinned tooling/CI checks.

Source schemas/protos remain identifiable and compatibility-tested.

## Architecture tests

CI SHOULD enforce relevant dependency boundaries, for example:

- domain cannot import adapters;
- CLI cannot mutate store/domain state except through application/runtime API;
- provider adapters cannot bypass gateway normalization;
- workers cannot access store implementation directly;
- integration protocols cannot become implicit internal ABI.

The exact tooling is language/build dependent.
