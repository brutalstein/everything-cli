# Versioning, Migrations, Distribution, and Release Safety

## 1. Objective

AER is a durable local runtime. Users will upgrade the binary while old projects, events, Engineering IR, policies, caches, SDKs, and possibly older clients still exist.

Compatibility is therefore a correctness and security subsystem, not release housekeeping.

ADR-0008 requires explicit compatibility contracts.

## 2. Independent version axes

Track separately:

```text
aer_binary_version
runtime_api_version
database_schema_version
event_schema_version
engineering_ir_schema_version
tool_abi_version
handoff_abi_version
config_schema_version
policy_schema_version
sdk_version
domain_profile_version
```

Do not overload one package semver to mean all of these.

## 3. Compatibility registry

Every release declares a machine-readable compatibility matrix:

```text
can_read_db
can_migrate_db_from[]
supported_event_versions[]
supported_ir_versions[]
runtime_api_min/max
tool/handoff_abi_min/max
config_versions[]
supported_provider_adapter_versions[]
```

Startup checks this before mutation.

## 4. Schema evolution

For durable/wire contracts:

- prefer additive evolution;
- never reuse removed field IDs/numbers where the encoding makes reuse dangerous;
- reserve/deprecate removed fields;
- distinguish binary-wire compatibility from JSON compatibility;
- validate unknown-field behavior explicitly;
- semantic meaning changes require a new version even if syntax still parses.

Protocol Buffers guidance is the baseline reference for protobuf evolution:
https://protobuf.dev/programming-guides/proto3/#updating

ProtoJSON has stricter compatibility caveats:
https://protobuf.dev/programming-guides/json/#json-wire-safety

## 5. Migration lifecycle

State migration sequence:

```text
detect version
 -> compatibility check
 -> preflight validation
 -> durable backup/checkpoint
 -> migration in controlled transaction/stages
 -> post-migration invariants
 -> mark migration complete
```

A failed migration MUST leave either the old valid state or an explicit recoverable migration state.

Never partially reinterpret historical events silently.

## 6. Event history

Historical events remain immutable.

If readers need a new representation, use:

- version-aware decoders,
- projection migration,
- explicit derived/upcast view.

Do not rewrite event history simply to make new code convenient.

## 7. Downgrade policy

Downgrade is not assumed safe.

If an older binary cannot safely read newer state it MUST:

- refuse write mode,
- explain the incompatibility,
- preserve data,
- offer documented restore/export paths.

Do not perform lossy automatic downgrade.

## 8. Daemon/client handshake

CLI/daemon connection negotiates:

```text
client_version
runtime_api_version
supported_features
minimum_compatible_version
```

Incompatible clients fail with an actionable message rather than mysterious RPC errors.

## 9. Release channels

Recommended channels:

```text
stable
beta
nightly/development
```

Project state records which binary performed migrations.

Enterprise policy MAY pin versions/channels and disable automatic update checks.

## 10. Secure distribution

A tool-executing daemon is high-value supply-chain surface.

Release artifacts SHOULD be:

- built in controlled CI,
- hashed,
- signed/attested,
- published with platform identity,
- verified before installation.

AER update metadata SHOULD use a mature secure-update framework or equivalent design rather than a homemade “download latest URL” mechanism.

Current TUF specification:
https://theupdateframework.io/spec/

TUF explicitly addresses rollback/freeze/mix-and-match update threats:
https://theupdateframework.io/docs/security/

## 11. Anti-rollback and freshness

The updater MUST NOT accept an older release merely because it is correctly signed when policy disallows downgrade.

Update metadata needs freshness/expiry and version monotonicity semantics.

Offline/manual installation remains possible but reports signature/version status.

## 12. Rollback

Binary rollback and state rollback are distinct.

Safe release process SHOULD preserve:

- pre-migration state backup,
- old binary identity,
- migration report,
- object-store compatibility.

If migration is irreversible, the CLI must state that before applying it and require policy-appropriate confirmation.

## 13. Release gates

Before stable release:

- supported upgrade paths tested from representative previous versions;
- migration crash-injection tests pass;
- CLI/daemon compatibility matrix tests pass;
- state replay remains valid;
- package/signature verification passes;
- SBOM/provenance generated when release policy requires;
- platform smoke tests pass;
- no critical architecture/security regression.

## 14. Self-update is not self-evolution

Policy Lab may produce candidate orchestration policy versions.

The release/update subsystem distributes approved artifacts.

A running model may not bypass release signatures/migrations because it “knows” a newer file should be installed.
