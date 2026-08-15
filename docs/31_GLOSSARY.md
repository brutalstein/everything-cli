# Glossary

**AER** — Adaptive Engineering Runtime, working project name.

**Agent harness** — Runtime/environment around a model that supplies tools, state, execution loop, persistence and control logic.

**Architecture Health** — Time-series signals representing maintainability, dependency structure, complexity concentration and other structural qualities.

**Artifact** — Immutable or content-addressed stored object such as tool output, patch, trace or report.

**Capability Registry** — Declared and empirically observed model/tool/sandbox capabilities.

**Cognitive Adapter** — Model-specific compiler from AER's semantic Handoff Envelope to provider/model-efficient instructions and context formatting.

**Compatibility Registry** — Machine-readable supported version ranges and migration relationships across AER's durable/wire contracts.

**Context Pack** — Versioned, bounded, provenance-bearing context selected for one model invocation or handoff.

**Domain Profile** — Composable declaration of project-domain capabilities and verification evidence, e.g. web UI, backend, CLI/TUI, systems, ML/data or IaC.

**Engineering IR** — Canonical model-independent representation of project intent, requirements, constraints, invariants and acceptance criteria.

**Engineering State** — Structured durable facts, decisions, assumptions, hypotheses, failures and progress derived from event/evidence history.

**Environment Fingerprint** — Versioned identity of OS/architecture/toolchain/lockfiles/sandbox/services/hardware properties relevant to reproducibility and evidence reuse.

**Evidence** — Machine-checkable observation tied to repository/environment/command identity.

**Handoff ABI** — Typed internal protocol for transfer between workers/models.

**Proof Manifest** — Mapping from requirement to implementation locations and verification evidence.

**Provider Gateway** — Normalized model-provider execution boundary handling streaming, retries, errors, rate limits, structured output, health and cancellation.

**Research Artifact** — Provenance-, time-, confidence-, and contradiction-aware representation of external research claims.

**Repo Snapshot** — Identity of the exact repository/workspace state a context/evidence item refers to.

**Resource Governor** — Admission/backpressure authority for workers, provider calls, subprocesses, services, memory/disk/network and cost budgets.

**Router Regret** — Difference between routing outcome and best eligible alternative on tasks where counterfactual outcomes are known/measured.

**Sandbox** — Constrained execution environment limiting filesystem, network, credential and resource authority.

**SpecDelta** — Versioned change to Engineering IR that can invalidate/replan tasks.

**Structural Erosion** — Increasing concentration of complexity in already complex units during iterative development.

**Task Envelope** — Typed definition of one unit of engineering work.

**Verifier Composition** — Risk/task/domain-specific collection of deterministic and model-based checks used for acceptance.

**Workspace Snapshot** — Identity of user-owned repository state including base commit and dirty changes before AER creates isolated writable execution state.
