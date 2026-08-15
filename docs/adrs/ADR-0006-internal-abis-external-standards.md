# ADR-0006: Internal Typed ABIs, External MCP/A2A Adapters

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

Use AER-specific typed Tool/Handoff contracts internally. Expose MCP 2026-07-28 and A2A v1.0 through adapters/gateways.

## Rationale

External standards provide ecosystem interoperability but should not dictate the hot-path internal semantic state, evidence model, or authority system.
