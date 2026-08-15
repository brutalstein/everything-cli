# ADR-0004: Verification Is an Independent Authority

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

A generator's claims and generator-writable visible tests cannot be the sole acceptance authority. Verification uses independent deterministic and, where appropriate, heterogeneous semantic signals.

## Consequences

- evidence/proof semantics are core domain concepts;
- held-out/immutable verification is required for high-risk/integrity-sensitive work;
- policy changes to verifiers follow the Policy Lab lifecycle.
