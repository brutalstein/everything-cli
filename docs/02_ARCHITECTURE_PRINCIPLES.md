# Architecture Principles

This document is normative.

## P1 — Contracts over conversations

Human conversation is input evidence, not the internal state format.

Material user intent MUST be compiled into versioned Engineering IR. Agent handoffs MUST use typed state. Raw transcripts MAY be retained for audit and semantic re-checking but SHOULD NOT be forwarded as the default inter-agent context.

## P2 — Models are replaceable compute

No business-critical invariant may depend on one provider's undocumented prompt behavior.

Provider adapters MUST normalize:

- model identity and version,
- capabilities,
- structured output,
- tool calls,
- reasoning/effort controls where available,
- token/cache accounting,
- rate limits,
- streaming,
- safety/error categories.

## P3 — Deterministic mechanisms dominate where possible

Do not ask an LLM to do what a deterministic tool can do more reliably and cheaply.

Examples:

- parse syntax with Tree-sitter / compiler APIs;
- resolve symbols with LSP where available;
- format with the language formatter;
- inspect git with git;
- calculate hashes deterministically;
- run tests instead of asking whether code “looks correct.”

## P4 — Context is a scarce resource

Every context item has cost, relevance, freshness, provenance, and expected utility.

The system MUST avoid context accumulation as a default behavior. Retrieval SHOULD be just-in-time and progressive. Context compression MUST preserve source anchors.

## P5 — Single agent first

A task starts as a single-worker candidate.

Parallel agents MAY be introduced only when:

- the task graph exposes independent work,
- expected parallel gain exceeds coordination cost,
- write-set overlap is acceptably low,
- integration verification exists,
- and isolated workspaces are available.

## P6 — Separate proposing from judging

A generator may propose code, plans, or even changes to AER's own policies. It MUST NOT be the sole authority that credits its own output.

Verification must have at least one independent signal. High-risk work requires heterogeneous or deterministic verification.

## P7 — Evidence outranks narrative

Statements such as “tests pass,” “migration works,” or “latency improved” are not facts unless tied to executable evidence.

Authoritative engineering state MUST retain provenance.

## P8 — Preserve long-horizon maintainability

A passing patch that creates severe structural erosion is not automatically a successful patch.

Architecture-health deltas are first-class acceptance signals.

## P9 — Fail closed at trust boundaries

Untrusted model output is data until validated.

Filesystem, network, credentials, external tools, verifier assets, and policy mutation are trust boundaries. Ambiguous authorization MUST fail closed.

## P10 — Event-derived durable state

Important transitions MUST be journaled append-only. Materialized views MAY be rebuilt from the journal plus content-addressed artifacts.

This enables replay, debugging, audit, and policy evaluation.

## P11 — Learning starts with observation

No learned router, self-evolving policy, or autonomous skill promotion should be introduced before telemetry and offline evals can measure whether it helped.

## P12 — Progressive autonomy

Autonomy is a policy dimension, not a binary flag.

Examples:

- read-only repository inspection,
- write within isolated worktree,
- package installation with constrained network,
- git commit,
- remote push,
- external side effect.

Higher-impact actions require stronger policy and evidence.

## P13 — Local-first, scale-out later

The architecture MUST support a robust local CLI/runtime without distributed services. Interfaces SHOULD permit future remote workers and managed sandboxes.

Do not impose cloud-operational complexity on the first usable implementation.

## P14 — Cross-platform is an interface requirement

Linux, Windows, and macOS must be architectural targets. Strong isolation backends may differ by OS, but task, evidence, state, and model contracts MUST remain portable.

## P15 — Explicit uncertainty

The system MUST distinguish:

- verified fact,
- user decision,
- system decision,
- assumption,
- hypothesis,
- unresolved question,
- model suggestion.

These may not collapse into a single “memory” bucket.

## P16 — Reversible changes

Task execution SHOULD be isolated in git branches/worktrees and produce small logical commits. Integration MUST remain reversible until verification completes.

## P17 — Observability is part of correctness

Every major decision must be explainable after the run:

- why this model,
- why this context,
- why this budget,
- why this topology,
- why this verification strength,
- why accepted/rejected.

## P18 — Policies are versioned artifacts

Retrieval weights, routing rules, prompt templates, verifier compositions, and recovery policies MUST have immutable version IDs in run telemetry.
