# ADR-0009: External Research Is Evidence, Not Authority

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

External web pages, documentation, papers, issues, package metadata, search results, and remote agent/tool output are untrusted evidence inputs.

AER MUST convert research into provenance-bearing claims with source identity, observation time, confidence, contradiction state, and freshness semantics before those claims can influence authoritative Engineering IR or durable verified facts.

Retrieved text cannot grant permissions or become a user requirement merely because a model read it.

## Rationale

Research is necessary for current APIs, dependencies, standards, security information, and domain design. It is also temporally unstable and vulnerable to conflicting sources, stale pages, indirect prompt injection, and deliberately poisoned user-generated content.

Treating a model-written research summary as truth would violate AER's evidence-over-narrative principle.

## Consequences

- research tasks emit `ResearchArtifact`, not free-form authoritative memory;
- decision-critical claims prefer primary/official sources and local executable confirmation where possible;
- contradiction is preserved rather than averaged away;
- temporal claims carry `observed_at`/freshness information;
- research access remains within sandbox/network/data policy;
- high-risk research findings require stronger corroboration.

## Revisit trigger

Research ranking/corroboration policies may improve empirically, but source provenance and authority separation remain invariant.
