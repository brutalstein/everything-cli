# ADR-0003: Single-Agent Default, Dynamic Parallelism

**Status:** Accepted baseline  
**Date:** 2026-08-15

## Decision

Do not run a fixed team of agents. Start tasks with the minimal topology and introduce parallel isolated workers only when dependency/write-set/economic conditions justify it.

## Rationale

Multi-agent systems can improve selected long-horizon tasks but add coordination, context and merge costs. Git worktree/branch-and-merge is the preferred isolation primitive for parallel code changes.
