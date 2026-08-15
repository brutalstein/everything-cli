# ADR-0008: Explicit Compatibility and Migration Contracts

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

AER treats durable-state schemas and cross-process/wire contracts as independently versioned compatibility surfaces.

Database schema, event payloads, Engineering IR, runtime API, Tool ABI, Handoff ABI, configuration, policy artifacts, and other durable core contracts MUST evolve through explicit compatibility rules and tested migrations.

Upgrades MUST preflight compatibility before mutating durable state. Historical events MUST NOT be silently reinterpreted or destructively rewritten merely to fit a new binary.

## Rationale

A long-lived local runtime will inevitably open projects created by older AER versions and may connect clients/adapters of different versions. A single package version cannot safely encode all compatibility semantics.

Without explicit rules, a normal binary upgrade can invalidate proof, replay, caches, or user state even when the new code itself is correct.

## Consequences

- supported version ranges are machine-readable;
- migrations are crash-tested and staged with recoverable checkpoints/backups;
- downgrade is refused when unsafe rather than performed lossy;
- client/daemon compatibility is negotiated;
- semantic meaning changes require a version even when serialized syntax still parses;
- release/update safety is part of correctness.

## Revisit trigger

Specific encodings/transports may change, but explicit compatibility remains mandatory for any durable or cross-process authority boundary.
