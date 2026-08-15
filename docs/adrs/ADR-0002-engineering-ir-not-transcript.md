# ADR-0002: Engineering IR Is the Canonical Semantic State

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

Do not use the human conversation transcript or a generated mega-prompt as the project's canonical semantic state. Compile intent into versioned Engineering IR.

## Rationale

Structured semantics are diffable, selectively materializable, traceable to requirements/evidence and independent of model context windows.

## Consequences

- an Intent Engine and schema migration strategy are required;
- raw transcripts remain provenance artifacts but are not repeatedly forwarded;
- model-specific prompts become compiled views rather than sources of truth.
