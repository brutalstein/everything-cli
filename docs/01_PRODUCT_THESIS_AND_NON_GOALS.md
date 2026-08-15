# Product Thesis and Non-Goals

## Thesis

Frontier models are becoming interchangeable, rapidly improving reasoning engines. Their capabilities matter, but durable software-engineering performance increasingly depends on the **harness**: environment design, context management, execution feedback, verification, isolation, and long-horizon state.

AER's thesis is:

> The valuable layer is not a larger prompt or a larger team of agents. It is a control system that allocates intelligence, context, tools, compute, autonomy, and verification according to the engineering state of the task.

The system should make a user able to say:

> “Build this product.”

without requiring that user to be an expert prompt engineer, model router, software architect, or agent operator.

## Product promise

AER should eventually provide:

1. **High-fidelity intent capture** — resolve material ambiguity before it contaminates implementation.
2. **Model-agnostic execution** — use the best available model for each decision, not one vendor everywhere.
3. **Token-efficient context** — give each model the smallest sufficient evidence-bearing context.
4. **Adaptive orchestration** — choose zero, one, or multiple agents based on expected utility.
5. **Long-horizon coherence** — preserve decisions, failures, invariants, and progress across context resets and sessions.
6. **Proof-carrying changes** — accepted work is accompanied by evidence mapped back to requirements.
7. **Architecture preservation** — prevent iterative agent work from gradually turning a codebase into unmaintainable “slop.”
8. **Measured self-improvement** — orchestration policies may evolve only through reproducible evaluation.

## Target users

### Primary

- engineers who want autonomous project implementation,
- technical founders building production systems,
- teams operating multiple model providers,
- organizations that need auditable AI-generated changes.

### Secondary

- researchers studying coding-agent orchestration,
- platform teams building internal engineering agents,
- enterprises that need policy-controlled autonomous coding.

## Non-goals

AER is not, initially:

- a foundation-model training project;
- a general-purpose personal assistant;
- an IDE replacement;
- a visual no-code workflow builder;
- a cloud-only platform;
- a benchmark-only SWE-bench solver;
- a fixed “AI software company” role-play system;
- a marketplace of thousands of unverified skills;
- an autonomous production deployment system with unlimited authority;
- an architecture whose value depends on secret prompts.

## Strategic moat candidates

The likely defensible assets are not the shell commands or chat UI. They are:

1. **Engineering IR schema and compiler behavior**
2. **Context utility and budget-allocation policy**
3. **Execution-grounded model capability registry**
4. **Routing / escalation policy learned from real outcomes**
5. **Typed handoff protocol + model-specific cognitive adapters**
6. **Evidence graph and proof-manifest semantics**
7. **Longitudinal architecture-health dataset**
8. **Failure trajectory and recovery dataset**
9. **Evaluation corpus for harness changes**

## Success metric hierarchy

### North-star metric

`Verified Engineering Outcome / Total Normalized Cost`

The numerator must represent accepted requirements that survive held-out verification and later regression, not model self-ratings.

### Supporting metrics

- requirement completion rate,
- hidden/held-out verifier pass rate,
- regression rate after subsequent tasks,
- architecture-health delta,
- median cost per accepted task,
- input/output/cache token usage,
- wall-clock time,
- human intervention rate,
- router regret,
- context precision / recall / yield,
- repeated-exploration rate,
- revert / rework rate,
- security-policy violations.

## Product principle: complexity must be invisible to normal users

Internally AER can maintain DAGs, context packs, evidence graphs, budgets, specialist models, and sandboxes.

Externally the normal flow should remain:

```text
$ aer
What do you want to build?
> ...

[Only high-information product questions are asked.]

✓ intent resolved
✓ engineering contract created
✓ implementation started
```

Advanced users MAY inspect and override policies, but ordinary users SHOULD NOT need to orchestrate agents manually.
