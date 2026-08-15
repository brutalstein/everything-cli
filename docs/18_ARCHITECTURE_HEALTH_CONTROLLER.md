# Architecture Health Controller

## Why this exists

Long-horizon agent code can continue passing tests while becoming more verbose, more coupled, and structurally harder to extend. Prompting “write clean code” is insufficient.

AER therefore monitors codebase health as a time series.

## Health dimensions

Language/tool-specific adapters should measure relevant subsets of:

- cyclomatic/cognitive complexity,
- complexity concentration / structural erosion,
- code duplication,
- file/function/class growth,
- dependency cycles,
- layer/boundary violations,
- fan-in/fan-out,
- public API surface growth,
- dead/unreachable code,
- test fragility,
- dependency count and risk,
- generated abstraction count,
- documentation/contract drift.

Do not pretend one aggregate score captures all maintainability.

## Baseline and delta

For each accepted task:

```text
health_delta = after_metrics - before_metrics
```

Acceptance policy considers the delta, not generic industry thresholds alone.

A pre-existing large file should not block every unrelated patch. A patch that materially worsens it should trigger review.

## Structural erosion

Track how total complexity mass concentrates in highly complex units over successive project iterations. This catches the tendency to keep appending logic to existing hotspots.

## Verbosity / redundancy

Measure duplicated or unnecessary implementation growth where tooling permits. Compare behavior delivered per changed code surface over time.

## Architecture boundaries

Projects may declare machine-readable boundaries:

```yaml
layers:
  - domain
  - application
  - infrastructure
rules:
  - from: domain
    may_depend_on: []
  - from: application
    may_depend_on: [domain]
```

Boundary violations become deterministic evidence.

## Refactoring trigger

The controller may create a refactoring task when:

- health regression exceeds policy threshold,
- repeated work concentrates in a hotspot,
- new feature implementation cost increases due to prior agent decisions,
- dependency graph develops cycles.

Refactoring must itself be verified against behavior.

## Architectural debt budget

Some changes legitimately add temporary complexity. Allow explicit, time-bounded debt records:

```text
debt_id
reason
owner/task
metric regression
expiry/trigger
planned remediation
```

Silent debt is not allowed.

## Metrics for AER itself

AER's own codebase MUST be subject to the same health controller. The orchestration product cannot tolerate architecture erosion in its core runtime.
