# Open Questions and Decision Gates

This blueprint deliberately freezes principles while leaving implementation choices open where real benchmarks are needed.

A coding agent MUST NOT arbitrarily close these questions without an ADR and evidence.

## Q1 — Local RPC transport

**Baseline candidate:** Protocol Buffers + gRPC over loopback.  
**Alternatives:** JSON-RPC/HTTP, Connect, custom framed protocol.  
**Decision gate:** prototype CLI↔daemon streaming, cancellation, Windows/Linux support and binary/API evolution.

## Q2 — Default vector index

**Requirement:** persistent, local, cross-platform, replaceable.  
**Decision gate:** benchmark build/update/search latency and operational complexity on representative repositories.

Vector search is optional for first lexical/structural milestone.

## Q3 — Lexical engine

Candidates: SQLite FTS5, Tantivy, direct ripgrep + ranking.  
Select using repository-scale and incremental-update benchmarks.

## Q4 — Strong local sandbox backends by OS

The contract is frozen; implementation backend is not.

Decision gates:

- Linux filesystem/network isolation strength and startup cost,
- Windows native vs WSL2/microVM usability,
- macOS isolation practicality,
- package-install/network broker integration.

## Q5 — Router learning algorithm

Do not choose before execution history exists.

Candidate family:

- contextual bandit,
- value model,
- Thompson/UCB-style safe exploration.

Decision metric: verified quality/cost regret with safety constraints.

## Q6 — Context fusion weights

Start with interpretable RRF + heuristics. Learn only after ContextBench data exists.

## Q7 — Architecture-health tooling per language

Need adapters and calibrated deltas. Avoid one universal metric.

## Q8 — Remote execution architecture

Not a v0.x blocker. Decide only after local scheduling/sandbox contracts are stable.

## Q9 — A2A exposure model

Whether AER acts primarily as A2A client, server, or both depends on enterprise integration demand. Keep gateway isolated from internal ABI.

## Q10 — Self-evolution autonomy

Baseline is offline proposal/eval/promotion. Any future autonomous canary promotion requires a separate security/evaluation ADR.
