# Coding-Agent Implementation Protocol

This file tells Claude Code, Codex, or another coding agent how to implement this blueprint safely and incrementally.

## Before every implementation task

1. Read `00_READ_ME_FIRST.md`.
2. Read the relevant roadmap phase.
3. Load only domain docs relevant to the task.
4. Inspect existing code and accepted ADRs before proposing new structure.
5. Identify the architecture contracts the task may touch.
6. Define executable acceptance checks before large edits.

## Do not “finish the whole project” in one pass

This project deliberately has interacting control planes. Large speculative scaffolding will create architecture drift.

Implement one vertical slice at a time and verify it.

## Mandatory workflow

```text
UNDERSTAND
  ↓
LOCATE CONTRACTS
  ↓
DEFINE ACCEPTANCE
  ↓
IMPLEMENT SMALLEST COHERENT SLICE
  ↓
RUN DETERMINISTIC TESTS
  ↓
RUN DOMAIN VERIFICATION
  ↓
CHECK ARCHITECTURE DELTA
  ↓
UPDATE DOC/ADR ONLY IF SEMANTICS CHANGED
  ↓
COMMIT LOGICAL UNIT
```

## Architectural restraint

Do not add:

- distributed queues,
- Kubernetes,
- graph databases,
- generic workflow DSLs,
- dozens of agent personas,
- heavy ML infrastructure,
- unneeded plugin systems,

unless the current roadmap phase and measured requirements justify them.

## Tests before abstractions

Before designing a generic interface, create at least two realistic implementations/use cases or a clearly documented future boundary. Avoid “future-proof” abstractions without pressure from the architecture.

Exceptions are explicit strategic boundaries already required by this blueprint, such as provider and sandbox adapters.

## Determinism

Core state transitions, task scheduling invariants, evidence identity, and policy authorization should be deterministic and unit/property tested.

Models should not decide internal persistence invariants.

## Dependency policy

Every new dependency needs:

- purpose,
- maintenance/security assessment,
- license compatibility,
- why standard library/existing dependency is insufficient.

Avoid dependencies that become architectural lock-in for trivial convenience.

## Cross-platform requirement

Do not merge OS-specific assumptions into the core domain.

OS-specific implementations belong behind adapter traits/modules and require explicit capability reporting.

## Errors

Use typed errors at subsystem boundaries. Preserve causal chain. Never convert important command/model errors into generic strings too early.

## Logging

Log structured events with IDs. Avoid dumping entire prompts/source/secrets by default.

## Documentation updates

If implementation discovers that a documented architectural decision is wrong:

1. do not silently diverge;
2. write/propose an ADR explaining evidence;
3. update affected docs after ADR acceptance;
4. preserve migration/backward compatibility where required.

## Definition of done for a code task

- intended contract implemented,
- tests added/updated,
- relevant tests pass,
- format/lint/typecheck pass where applicable,
- no unapproved architecture change,
- security boundary preserved,
- event/evidence semantics maintained,
- docs updated only when behavior/contracts changed.

## When uncertain

Prefer a small research spike with evidence over a large implementation based on guesswork.
