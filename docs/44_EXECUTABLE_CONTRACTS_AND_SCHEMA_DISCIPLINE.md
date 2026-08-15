# Executable Contracts and Schema Discipline

## 1. Objective

AER repeatedly states that contracts outrank conversations. That claim is only meaningful if high-authority data structures are executable, versioned, and semantically validated.

This document closes the gap between prose architecture and machine-enforceable contracts.

## 2. Core contract registry

The following are first-class contracts:

```text
EngineeringIR
TaskEnvelope
RunState
Budget
ContextPack
HandoffEnvelope
WorkResult
EvidenceRecord
ProofManifest
ResearchArtifact
EnvironmentFingerprint
ModelCapabilityRecord
PolicyArtifact
RunEvent
Configuration
```

Initial JSON Schemas live under `docs/schemas/`. Runtime Rust/protobuf types may become the implementation authority, but generated/serialized representations MUST remain compatibility-tested against these semantics.

## 3. Two validation layers

### Structural validation

JSON Schema / protobuf decoding verifies:

- fields,
- types,
- enums,
- required structure,
- bounds.

### Semantic validation

Deterministic domain validators verify cross-object rules such as:

- referenced requirement IDs exist,
- task dependency graph is acyclic,
- evidence repo snapshot matches the verified change,
- accepted proof references passing required evidence,
- policy versions exist,
- ContextPack sources resolve,
- budgets obey parent/org caps.

Schema success alone never means semantic validity.

## 4. Strictness

At authority boundaries, mature contracts SHOULD reject unknown top-level fields rather than silently accepting typos.

Where extension is necessary, provide an explicit namespaced `extensions` field.

Avoid broad unconstrained `{ "type": "object" }` fields for security/state-critical semantics.

Some provider capability metadata may remain extensible by design; it is not automatically authoritative.

## 5. Version field

Every durable core contract requires a schema/protocol version either:

- inside the object,
- or unambiguously from its event/envelope/container.

Do not infer versions from file names alone.

## 6. References

References use stable IDs, not copied prose.

Cross-reference validation runs in CI/runtime according to authority:

```text
REQ -> AC -> Task -> Evidence -> Proof
Policy -> Run
ContextPack -> RepoSnapshot/IR
ResearchClaim -> Source
```

Dangling references fail validation unless an explicit retention tombstone type permits them.

## 7. Compatibility

Follow `40_VERSIONING_MIGRATIONS_AND_RELEASE_SAFETY.md`.

Schema CI maintains fixtures for:

- oldest supported version,
- current version,
- forward additive fields where expected,
- invalid/breaking examples,
- migration round trips.

## 8. Generated bindings

If code generation is used:

- generated files are reproducible,
- generator/tool version is pinned,
- CI fails on stale generated output,
- generated code is not hand-edited,
- source schema remains identifiable.

## 9. Missing-schema rule

A new architecture document introducing a durable or cross-process object MUST either:

1. add an executable schema/type and tests, or
2. explicitly name the existing contract it extends, or
3. mark the object experimental/non-authoritative with an ADR/open decision.

Do not create permanent “typed” objects only in Markdown.

## 10. Schema quality tests

CI SHOULD include:

- JSON Schema meta-validation,
- example validation,
- cross-reference fixtures,
- property-based serialization round trips,
- unknown-field tests,
- numeric/budget boundary tests,
- compatibility fixture tests,
- fuzzing of deserializers for high-trust boundaries.

## 11. New executable baseline

This audit adds schemas for:

- `budget.schema.json`
- `run.schema.json`
- `context-pack.schema.json`
- `work-result.schema.json`
- `proof-manifest.schema.json`
- `research-artifact.schema.json`
- `environment-fingerprint.schema.json`
- `policy-artifact.schema.json`

These are architecture baselines, not permission to scaffold every implementation before its roadmap phase.
