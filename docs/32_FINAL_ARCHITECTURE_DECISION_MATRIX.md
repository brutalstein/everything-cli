# Final Architecture Decision Matrix

This file summarizes the baseline decisions a coding agent should treat as settled unless evidence justifies an ADR.

| Area | Baseline decision | Rejected default | Why |
|---|---|---|---|
| Product category | Adaptive software-engineering runtime | Prompt wrapper | Durable value is harness/control plane |
| Core | Rust local-first library + daemon + CLI | Python monolith | Safety, cross-platform runtime, typed contracts |
| Semantics | Engineering IR | Mega-prompt / transcript | Versioned, diffable, selective, model-independent |
| Requirements | Dedicated intent/spec phase | Code immediately | Avoid propagating ambiguous intent |
| Model strategy | Multi-provider capability registry | One fixed “best model” | Models differ by task/cost and change rapidly |
| Routing | Feedback-aware + scout/escalate | Static prompt classifier | Repository evidence improves decisions |
| Context | Hybrid multi-view retrieval | Embedding-only RAG | No retrieval family dominates |
| Context budget | Utility/token + coverage | Fill context window | Cost and attention are scarce |
| Compression | Extractive/source-anchored first | Free-form summary as truth | Preserve fidelity/provenance |
| Memory | Evidence-gated engineering state | Chat memory | Prevent stale/unverified narrative becoming authority |
| Agent topology | Single by default, dynamic | Fixed role swarm | Coordination has real cost |
| Parallel code work | Git worktrees + branches | Shared writable tree | Isolation and reversible integration |
| Handoff | Typed ABI | Free-form agent chat | Reduce drift/rediscovery/token overhead |
| Tools | Internal Tool ABI + adapters | Every operation through giant tool schema | Efficiency + policy control |
| External tools | MCP 2026-07-28 | Proprietary-only plugin layer | Ecosystem interoperability |
| External agents | Optional A2A v1.0 gateway | A2A internal bus | External standard without internal overhead |
| Sandbox | Filesystem + network + credential isolation | Permission spam / unrestricted host | Strong autonomy boundary |
| State store | SQLite WAL + event journal + object store | Early distributed DB/event bus | Local-first durability and simplicity |
| Verification | Multi-layer independent authority | Visible tests only | Reward hacking and semantic gaps |
| Acceptance artifact | Proof Manifest | “Agent says done” | Requirement-level auditability |
| Code quality | Continuous architecture-health delta | Final “clean code” prompt | Iterative agent code structurally degrades |
| Observability | OpenTelemetry + AER events | Ad-hoc logs | Learnable, auditable orchestration |
| Self-improvement | Offline candidate→eval→shadow→canary | Live self-editing agent | Separate proposing from crediting |
| Scaling | Local first, remote workers later | Kubernetes-first | Avoid premature operational complexity |

## Key architecture boundary

The following are **pluggable policy/adapter decisions**, not permanent foundations:

- exact model vendors,
- exact embedding model,
- exact vector index,
- exact context fusion weights,
- exact learned router algorithm,
- exact sandbox backend per OS,
- exact model prompt template.

The following are **semantic foundations**:

- Engineering IR,
- task/evidence/state identities,
- verifier independence,
- policy versioning,
- sandbox authority model,
- event-backed durability,
- typed handoff,
- proof-carrying acceptance.

That distinction is essential to keep AER flexible as the model ecosystem changes.
