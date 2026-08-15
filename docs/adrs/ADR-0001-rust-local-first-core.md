# ADR-0001: Rust Local-First Core

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

Implement the durable core runtime and CLI/daemon foundation in Rust, with external language SDKs added through stable protocol boundaries.

## Rationale

The core executes untrusted/generated processes, manages concurrent durable state and must run efficiently on developer machines across operating systems. Rust offers a strong fit for memory safety, typed domain contracts, concurrency and distributable binaries.

The deeper decision is not “Rust forever”; it is that **core invariants do not depend on Python application state or a single model SDK**.

## Consequences

- provider SDK gaps may require direct HTTP clients or thin adapters;
- Python remains appropriate for experimental ML policies/evals;
- core domain types must remain decoupled from async/provider/database implementation details.

## Revisit trigger

Only revisit if implementation evidence shows unacceptable ecosystem/portability cost that cannot be solved by adapters.
