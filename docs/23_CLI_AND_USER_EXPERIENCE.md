# CLI and User Experience

## Principle

Internal sophistication must not become user-facing orchestration burden.

The default interface is goal-oriented and conversational. Advanced introspection is available when needed.

## Working command

This blueprint uses `aer` as a placeholder binary name.

## Core commands

```text
aer init
aer
aer build "<goal>"
aer resume
aer status
aer stop
aer doctor
```

## Inspection commands

```text
aer inspect project
aer inspect task <id>
aer inspect context <id>
aer inspect route <id>
aer inspect evidence <id>
aer inspect proof <id>
aer inspect cost [run]
aer inspect health
aer inspect events [run]
```

## Model/policy commands

```text
aer models
aer models benchmark
aer policy show
aer config get
aer config set
aer eval run <suite>
```

## Default project flow

```text
$ aer

What do you want to build?
> A realtime meeting assistant that extracts decisions and tasks...

AER: One product behavior materially changes the architecture:
Should analysis happen live during the meeting, after upload, or both?

> both

AER: Understood. I can make the remaining infrastructure choices from engineering defaults.

✓ Specification compiled
✓ 12 required behaviors
✓ 4 quality constraints
✓ 2 explicit non-goals
✓ 1 assumption recorded

Start implementation? [Y/n]
```

User confirmation at this point is configurable. In fully autonomous trusted workflows it may auto-start.

## Normal progress UI

Avoid printing raw model thought or every shell line.

Show semantic progress:

```text
Project: meeting-assistant
Run: 01J...

[accepted] specification contract
[accepted] repository/bootstrap architecture
[running ] realtime ingestion
[ready   ] persistence layer
[blocked ] UI integration — waits for API contract

Verification: 31/34 required checks currently passing
Cost: $4.82 | input 1.2M | output 91K | cache hit 68%
```

## Explainability

`aer inspect route TASK-42` should answer:

- eligible models,
- selected model,
- relevant empirical stats,
- expected cost/quality,
- escalation history.

`aer inspect context TASK-42` should show source refs and ranking reasons, not hidden reasoning.

## Configuration philosophy

Normal users configure intent such as:

```yaml
quality_mode: balanced
security_profile: sandboxed
autonomy: workspace
```

They should not need to hand-write complex agent graphs.

## Headless mode

CI/automation requires:

```text
aer build --non-interactive --spec project.yaml
aer status --json
aer proof --json
```

If non-interactive input contains unresolved high-impact ambiguity, fail with a structured `needs_input` result unless policy defines a safe default.

## Exit codes

Define stable exit categories:

```text
0 success/accepted
2 needs user input
3 verification failed
4 policy/security blocked
5 environment/setup failure
6 budget exhausted
7 internal runtime failure
```

Exact numbering may be finalized in an ADR.
