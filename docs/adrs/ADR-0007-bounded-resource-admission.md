# ADR-0007: Bounded Resource Admission Is a Core Runtime Invariant

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

All model calls, workers, sandboxes, subprocesses, ephemeral services, and high-volume internal streams MUST enter through explicit bounded admission/resource-control paths.

The daemon MUST NOT contain unbounded work queues whose growth depends on model behavior or task-graph parallelism.

Verification capacity MUST be protectable from generator saturation.

## Rationale

AER dynamically changes topology, context, models, and recovery strategy. Without a Resource Governor, those adaptive mechanisms can multiply demand faster than a local machine or provider quota can absorb. Resource exhaustion then becomes a correctness problem: leases expire, verification starves, retries synchronize, disks fill, and apparently independent tasks interfere.

Backpressure is therefore part of orchestration semantics, not a later performance optimization.

## Consequences

- a task becoming `running` requires a lease plus admitted resource capacity;
- every queue/channel declares capacity and overflow/backpressure behavior;
- provider quota/rate-limit state participates in admission;
- cancellation and crash recovery must release/reconcile reservations;
- high-volume non-authoritative UI deltas may be coalesced, while durable state transitions may not be silently dropped;
- resource decisions are observable and policy-versioned.

## Revisit trigger

The admission algorithm may evolve with measured workload, but bounded ownership and backpressure remain invariant unless a future ADR provides an equally strong correctness model.
