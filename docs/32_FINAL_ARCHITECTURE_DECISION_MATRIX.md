# Final Architecture Decision Matrix

This file summarizes baseline decisions a coding agent should treat as settled unless evidence justifies an ADR.

| Area | Baseline decision | Rejected default | Why |
|---|---|---|---|
| Product category | Adaptive software-engineering runtime | Prompt wrapper | Durable value is harness/control plane |
| Core | Rust local-first library + daemon + CLI | Python monolith | Safety, cross-platform runtime, typed contracts |
| Semantics | Engineering IR | Mega-prompt / transcript | Versioned, diffable, selective, model-independent |
| Requirements | Dedicated intent/spec phase | Code immediately | Avoid propagating ambiguous intent |
| External research | Provenance/freshness-bearing Research Artifacts | Model-written web summary as truth | External information is unstable/untrusted evidence |
| Model strategy | Multi-provider capability registry | One fixed “best model” | Models differ by task/cost and change rapidly |
| Provider runtime | Normalized bounded retry/rate-limit/circuit/failover gateway | Direct provider SDK calls throughout core | Operational failures need consistent semantics |
| Routing | Feedback-aware + scout/escalate | Static prompt classifier | Repository evidence improves decisions |
| Context | Hybrid multi-view retrieval | Embedding-only RAG | No retrieval family dominates |
| Context budget | Utility/token + coverage | Fill context window | Cost and attention are scarce |
| Compression | Extractive/source-anchored first | Free-form summary as truth | Preserve fidelity/provenance |
| Memory | Evidence-gated engineering state | Chat memory | Prevent stale/unverified narrative becoming authority |
| Agent topology | Single by default, dynamic | Fixed role swarm | Coordination has real cost |
| Resource scheduling | Bounded admission + backpressure + leases | Unbounded worker/task queues | Adaptive topology must not exhaust runtime/provider capacity |
| Parallel code work | Git worktrees + branches | Shared writable tree | Isolation and reversible integration |
| User workspace | Preserve dirty state; AER-owned isolated branches/worktrees | Reset/stash/overwrite user tree | User state is outside agent authority by default |
| Handoff | Typed ABI | Free-form agent chat | Reduce drift/rediscovery/token overhead |
| Tools | Internal Tool ABI + adapters | Every operation through giant tool schema | Efficiency + policy control |
| External tools | Current MCP adapter at boundary | Proprietary-only plugin layer | Ecosystem interoperability |
| External agents | Optional A2A gateway | A2A internal bus | External standard without internal overhead |
| Sandbox | Filesystem + network + credential isolation | Permission spam / unrestricted host | Strong autonomy boundary |
| State store | SQLite WAL + event journal + object store | Early distributed DB/event bus | Local-first durability and simplicity |
| Durable contract evolution | Explicit independent versions + tested migrations | “Package semver will handle it” | Old projects/events/clients must survive upgrades predictably |
| Distribution/update | Signed/attested releases + secure update metadata | Download-latest-and-run | Tool-executing daemon is high-value supply-chain surface |
| Environment identity | Environment Fingerprint + lock/toolchain identity | Repo commit alone | Evidence/reuse depends on execution environment |
| Dependencies | Pinned/provenance-aware install policy + SBOM hooks | Autonomous unconstrained package installs | Reproducibility and supply-chain safety |
| Verification | Multi-layer independent authority | Visible tests only | Reward hacking and semantic gaps |
| Domain verification | Composable capability/verification profiles | One generic test recipe | Web/CLI/backend/systems/ML/IaC require different evidence |
| Acceptance artifact | Proof Manifest | “Agent says done” | Requirement-level auditability |
| Code quality | Continuous architecture-health delta | Final “clean code” prompt | Iterative agent code structurally degrades |
| Data governance | Explicit sensitivity/retention/tenant scope | Keep everything forever | Prompts/source/traces/learning artifacts have different obligations |
| Observability | OpenTelemetry + AER events | Ad-hoc logs | Learnable, auditable orchestration |
| Self-improvement | Offline candidate→eval→shadow→canary | Live self-editing agent | Separate proposing from crediting |
| Scaling | Local first, remote workers later | Kubernetes-first | Avoid premature operational complexity |
| Executable contracts | Schema/type registry + semantic validators | Markdown-only “typed” objects | Architecture claims must be machine-enforceable |

## Key architecture boundary

The following are **pluggable policy/adapter decisions**, not permanent foundations:

- exact model vendors;
- exact embedding model/vector index;
- exact context fusion weights;
- exact learned router algorithm;
- exact provider retry constants;
- exact sandbox backend per OS;
- exact model prompt template;
- exact package/provenance tooling implementation;
- exact secure-update implementation;
- exact domain-profile tool adapters.

The following are **semantic foundations**:

- Engineering IR;
- task/evidence/state identities;
- verifier independence;
- policy versioning;
- sandbox authority model;
- event-backed durability;
- typed handoff/tool boundaries;
- proof-carrying acceptance;
- bounded resource admission/backpressure;
- external research as provenance-bearing evidence;
- environment/dependency identity;
- preservation of user-owned workspace state;
- explicit compatibility/migration lifecycle;
- data sensitivity/retention boundaries.

That distinction keeps AER flexible as models, providers, protocols, tools and deployment environments change.
