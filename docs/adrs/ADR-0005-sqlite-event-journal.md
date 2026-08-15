# ADR-0005: SQLite + Append-Only Journal for Initial Durable State

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

Use SQLite for initial local durable metadata/projections, plus append-only events and a content-addressed object store.

## Rationale

The local runtime needs transactions, crash recovery, inspectability and cross-platform simplicity. Distributed infrastructure is premature before local correctness is proven.

## Revisit trigger

Measured workload requiring multi-host coordination or storage throughput beyond the local architecture.
