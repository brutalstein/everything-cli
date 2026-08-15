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
│   ├── aer-protocol/          # runtime RPC schemas/types
│   ├── aer-store/             # SQLite journal/projections/object metadata
│   ├── aer-models/            # provider gateway + adapters
│   ├── aer-context/           # context pack/ranking/budget
│   ├── aer-repo/              # repository intelligence/indexing
│   ├── aer-exec/              # process/tool ABI
│   ├── aer-sandbox/           # sandbox backend abstraction
│   ├── aer-verify/            # verifier controller/evidence
│   ├── aer-health/            # architecture health adapters
│   ├── aer-telemetry/         # OTel + accounting
│   └── aer-integrations/      # MCP/A2A later
├── sdk/
│   ├── typescript/            # later
│   └── python/                # later
├── evals/
│   ├── fixtures/
│   ├── intent/
│   ├── context/
│   ├── routing/
│   ├── handoff/
│   ├── evolution/
│   └── security/
└── scripts/
```

## Dependency direction

```text
aer-domain
   ↑
aer-core
   ↑
application adapters (store/models/repo/exec/verify)
   ↑
daemon / CLI
```

`aer-domain` MUST NOT depend on provider SDKs, database drivers, sandbox libraries, or UI code.

## Domain package

Owns:

- IDs,
- Engineering IR types,
- task state machine,
- evidence semantics,
- policy decisions/value objects,
- facts/hypotheses,
- capability/security types.

Keep it deterministic.

## Adapter packages

Provider-specific implementation should remain shallow. A new model vendor should not require changes to task/evidence/context domain logic.

## Feature flags

Use feature flags sparingly for optional heavy integrations. Do not create combinatorial build matrices.

## Generated schema code

Generated Protocol Buffers/JSON Schema bindings belong in clearly marked generated directories and are regenerated through deterministic scripts/CI checks.
