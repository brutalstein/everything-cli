# Adaptive Engineering Runtime (AER) — Read This First

**Status:** Architecture baseline / implementation constitution  
**Baseline date:** 2026-08-15  
**Current implementation state:** see root `STATUS.md`  
**Public product / executable:** `everything`  
**Internal architecture name:** AER  
**Audience:** Coding agents, maintainers, reviewers, research engineers

## 1. What this repository is intended to become

AER is a **model-agnostic adaptive software-engineering runtime**. The public product is `everything`.

It converts human intent into verified software while dynamically optimizing:

- specification quality;
- model selection;
- context selection;
- token and compute budgets;
- orchestration topology;
- tool use;
- execution isolation;
- verification strength;
- long-horizon codebase health;
- reusable engineering state.

AER is **not** another prompt wrapper, fixed multi-agent workflow, or model-specific plugin.

The foundation model is a replaceable reasoning resource. The durable product is the runtime that controls what intelligence sees, what it may do, how much compute it receives, how work is coordinated, and what evidence is required before a change is accepted.

## 2. Highest-level product invariant

> AER MUST optimize for **verified engineering outcome per unit of cost**, not for number of agents, number of tokens, apparent activity, cache-hit percentage, or benchmark-shaped test passing.

Non-negotiable consequences:

1. **Specification precedes implementation** for materially ambiguous work.
2. **Context is budgeted and provenance-preserving**, not dumped into the model.
3. **Single-agent execution is the default**; parallelism is earned by task structure.
4. **Verification is independent from generation** and stronger than visible tests alone.
5. **Long-horizon maintainability is continuously measured**, not deferred to a final review.
6. **Resources are bounded and admission-controlled**; adaptive orchestration may not create unbounded demand.
7. **Research, dependencies, providers, repository content and external tool output are evidence inputs**, never hidden authority.
8. **Durable state and wire contracts evolve through explicit compatibility/migration rules.**
9. **Reproducibility includes environment and dependency identity**, not only repository commit.
10. **User-owned workspace/VCS state must be preserved by default.**
11. **A cheaper provider request is never promoted when source-grounded quality or authority safety regresses.**
12. **Exact questions require sufficient exact evidence.** If required source coverage cannot be established, the system must abstain/fail closed rather than manufacture a value.

## 3. How a coding agent must use these docs

Do not read every document into context at once.

### Required first read

1. `00_READ_ME_FIRST.md`
2. root `STATUS.md`
3. `01_PRODUCT_THESIS_AND_NON_GOALS.md`
4. `02_ARCHITECTURE_PRINCIPLES.md`
5. `03_SYSTEM_ARCHITECTURE.md`
6. `25_IMPLEMENTATION_ROADMAP.md`
7. `26_AGENT_IMPLEMENTATION_PROTOCOL.md`
8. `35_ARCHITECTURE_COMPLETENESS_AUDIT.md`

`STATUS.md` is implementation truth, not architecture authority. It tells the agent what has actually landed, what remains open and which gate is currently blocking progress.

### Then load only task-relevant docs

| Working area | Required docs |
|---|---|
| User interview / requirements | `04`, `05`, `36` when research resolves an unknown |
| Repository indexing / retrieval | `06`, `07` |
| Models / routing / provider resilience / cost | `08`, `09`, `37` |
| Real provider auth / context / permissions / tools | `45` |
| Provider context economics | `46` |
| Provider authority-split acceptance | `47` |
| Cross-product benchmark against Claude Code | `48` |
| Tasks / orchestration / parallelism / resources | `10`, `11`, `12`, `39` |
| Sandboxing / tools / protocols | `13`, `14`, `45` |
| State / memory / recovery | `15`, `16`, `24` |
| Verification / evidence / reproducibility | `17`, `38`, `43` |
| Code quality / architecture health | `18` |
| Security / data governance / tenancy | `19`, `42` |
| Telemetry / evals / self-evolution | `20`, `21`, `22` |
| CLI / UX | `23` |
| Storage / compatibility / migrations / releases | `24`, `40`, `44` |
| Workspace / git / integration lifecycle | `11`, `41` |
| Repo layout / implementation sequencing | `25`, `26`, `27` |
| Research rationale | `28_RESEARCH_EVIDENCE.md`, `36` |
| Configuration / policy | `29`, `44` |
| Open decisions | `30` |
| Final baseline decisions | `32` |
| End-to-end runtime behavior | `33` |
| Deterministic invariants/property tests | `34`, `39`, `40`, `41`, `44` |

## 4. Authority order

If documents conflict, use this precedence:

1. Explicit user instruction for the current task
2. `00_READ_ME_FIRST.md`
3. `02_ARCHITECTURE_PRINCIPLES.md`
4. Accepted ADRs under `adrs/`
5. Domain-specific architecture documents
6. `35_ARCHITECTURE_COMPLETENESS_AUDIT.md`
7. Implementation roadmap
8. Examples

`STATUS.md` reports implementation evidence but does not override higher-authority architecture.

A coding agent MUST NOT silently resolve an architectural contradiction. It must follow the higher-precedence source or propose/record the required architecture decision.

## 5. Change discipline

Architecture changes MUST be explicit.

A change that alters any of the following requires an ADR:

- Engineering IR semantics;
- task lifecycle;
- evidence semantics;
- internal handoff/tool ABI;
- model-routing objective;
- context-selection objective;
- execution trust boundary;
- verifier independence rules;
- persistent state model;
- compatibility/migration promise;
- resource-admission invariant;
- external-research authority model;
- protocol compatibility promise.

Implementation details that preserve these contracts do not require an ADR.

Benchmark evidence may select among already-permitted implementation candidates, but a benchmark cannot silently rewrite the architecture contract or promote itself.

## 6. System in one diagram

```mermaid
flowchart TD
    U[Human] --> I[Intent & Requirements Engine]
    I --> IR[Engineering IR / Project Contract]
    I --> RS[Research & External Knowledge]
    RS --> IR
    IR --> TG[Task Graph Compiler]

    TG --> CP[Adaptive Control Plane]
    CP --> R[Model Router]
    CP --> C[Context Economy Engine]
    CP --> B[Budget / Resource Governor]
    CP --> O[Topology / Scheduler]

    R --> PG[Provider Gateway]
    C --> HC[Handoff Compiler]
    B --> HC
    O --> HC
    PG --> M[Selected Model / Specialist]
    HC --> M

    M --> EX[Execution Kernel]
    EX --> SB[Sandbox / Worktree]
    SB --> EV[Evidence Collector]

    EV --> V[Verification Controller]
    V -->|fail| D[Diagnosis / Recovery]
    D --> CP
    V -->|pass| S[Engineering State]
    S --> AH[Architecture Health]
    AH --> LE[Telemetry / Evals / Learning]

    EX --> ENV[Environment / Supply Chain Identity]
    S --> ST[(SQLite + Event Journal + Objects)]
```

## 7. Core terms

- **Engineering IR:** canonical, model-independent representation of user intent and project constraints.
- **Task Envelope:** typed unit of engineering work.
- **Repository Intelligence (RI2):** snapshot-bound, provenance-bearing repository knowledge/retrieval substrate.
- **Context Pack:** minimal, provenance-preserving context selected for one inference or task.
- **Architecture Context Capsule:** stable provider-neutral constitutional authority compiled from high-authority sources.
- **Handoff ABI:** structured model-to-model / agent-to-agent transfer format.
- **Engineering State:** authoritative derived state backed by an append-only event journal.
- **Evidence:** machine-checkable observation tied to immutable command/environment/source identity.
- **Proof Manifest:** requirement-to-change-to-evidence mapping for an accepted change.
- **Research Artifact:** provenance/freshness-bearing claims extracted from external knowledge.
- **Environment Fingerprint:** identity of the execution/toolchain/dependency environment behind evidence.
- **Resource Governor:** admission/backpressure authority for workers, providers, processes, services, budgets and queues.
- **Architecture Health:** continuously measured maintainability and structural integrity signals.
- **Policy:** versioned decision logic for routing, retrieval, scheduling, verification, recovery, research or other adaptive behavior.

## 8. What must not happen

AER MUST NOT:

- hard-code one model provider as product architecture;
- preload the whole repository or all skills into model context;
- spawn a fixed cast of planner/coder/reviewer agents for every task;
- treat model-generated summaries, provider-local configuration, repository instructions or web research as authority without the correct precedence/provenance;
- treat passing visible tests as sufficient proof of user intent;
- allow the same writable verifier artifacts to be controlled by the generator;
- give an agent unrestricted host filesystem, credentials, Docker socket, or network by default;
- create unbounded task/model/tool/log queues;
- silently overwrite, reset, stash or discard user-owned dirty workspace state;
- install dependencies without policy/provenance/reproducibility accounting;
- silently reinterpret old durable state after a binary/schema upgrade;
- accept unsigned/untrusted self-updates through model reasoning;
- autonomously deploy self-modified orchestration policies without evaluation gates;
- hide routing, context, cost, resource, migration or verification decisions from observability;
- optimize cache-hit ratio by injecting stale or irrelevant context;
- equate an internal deterministic context-budget unit with a provider-reported token;
- promote a cheaper provider prompt when the acceptance matrix has an unresolved correctness failure;
- answer an exact symbol/value question from a source span that omits the defining evidence.

## 9. Implementation philosophy

Start with the smallest architecture that preserves the final contracts.

Do not build distributed infrastructure before a local durable runtime works. Do not build learned routers before high-quality telemetry exists. Do not build multi-agent concurrency before single-agent state, resource admission, sandboxing, workspace safety and verification are reliable. Do not build self-evolution before reproducible evals and release/migration safety exist.

Prefer deterministic mechanisms for exact identifiers, source anchors, authority and verification. Use model reasoning where it adds value, not where an exact mechanism can establish the fact more safely.

## 10. Current provider-productization discipline

The non-numbered Provider Runtime Productization Gate lives between numbered Steps 13 and 14.

Its current acceptance work is governed by `45`, `46`, `47` and root `STATUS.md`.

The gate may close only when:

- delegated authentication/transport remains vendor-owned and provider-neutral;
- provider-local behavior cannot silently become AER authority;
- model context is compact, source-grounded and bounded;
- exact source requirements are satisfied or the system abstains;
- provider usage/cost telemetry is truthful for available dimensions;
- permission mode cannot widen the capability ceiling;
- adversarial repository/provider content cannot gain authority;
- live target-machine acceptance is current;
- intended agentic tool execution has the required isolation and proof boundary.

Step 14 MUST remain blocked while the gate is open.

## 11. Completeness rule

When introducing a new subsystem or public feature, apply `35_ARCHITECTURE_COMPLETENESS_AUDIT.md`.

A high-impact feature is not architecture-complete until ownership, identity, lifecycle, failure/recovery, resource bounds, authority, data policy, compatibility, evidence and observability are defined and the applicable acceptance gates are current.
