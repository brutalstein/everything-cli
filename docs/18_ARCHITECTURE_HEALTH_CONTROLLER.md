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

## Current implementation truth

The controller lives in `crates/aer-health`. It is deliberately partial, and the partiality is the honest part.

### Dimensions that are measured

| Dimension | How |
|---|---|
| file size | addressable lines |
| unit count | units the language adapter identified |
| largest unit | lines in the biggest unit of a file |
| complexity concentration | share of a file's lines inside oversized units, in basis points |
| duplicated lines | line blocks of at least six normalized lines that repeat across the measured set |
| boundary violations | declared layer rules over the dependency graph |

Duplication is line-based and therefore heuristic: it sees copied text, not copied meaning, and renaming one identifier hides a block from it. It is normalized for whitespace so indentation neither creates nor hides a match, and it carries the same capability tier as the rest of the file's numbers rather than being presented as a semantic fact. It is scanned over a set of files, because a block is only duplicated relative to something else.

Concentration is the structural-erosion signal this document asks for: appending logic to an existing hotspot moves it even when the file barely grows, and it moves in the worsening direction even when unit count falls.

### Dimensions that are absent

Dead code, test fragility and documentation drift are listed above and remain unimplemented, for stated reasons rather than for lack of attention.

Dead code needs compiler truth. A syntax-tier guess at "nothing references this symbol" is wrong across trait implementations, macros, re-exports and cross-crate use, and the capability-tier system exists precisely so a guess is not dressed as a fact. The dimension arrives with a precise-semantic adapter, not before.

Test fragility needs run history — the same test passing and failing across runs without a corresponding change. The runtime does not yet retain enough verification history to compute it, and inventing a proxy from a single run would measure nothing.

Documentation drift needs a mapping from documented claims to the code that satisfies them. The repository has document integrity, which is a different thing: it proves documents are unmodified, not that they are still true.

A dimension that always reports zero would read as a clean bill of health it did not earn, so none of these is stubbed.

### Rules the implementation obeys literally

There is **no aggregate score**. A comparison returns a list of per-dimension findings and nothing else.

Acceptance reads the **delta**. A repository that already contains a large file produces no finding for a patch that does not touch it, and deleting code is never a regression.

Every finding carries the **capability tier** that produced it, so a measurement inferred from a syntax adapter is never presented as compiler truth. Measurement reuses the existing language capability registry through `aer_repo::measure_source` rather than growing a second idea of what a function is; a file whose parse reported an error degrades to the text tier instead of reporting partial symbols as if they were complete.

Debt records are **time-bounded and dimension-scoped**. A record excuses the dimension and path it names, up to the regression it names, until the revision it names. It excuses nothing else, and an expired record excuses nothing at all.

### AER measuring itself

`tools/aer-health-check` runs both checks against this repository and is wired into the repository gates alongside the documentation and contract checks.

The two checks differ in kind on purpose. The **layer gate** is absolute and blocking: crate layering is a declared architectural fact, so a dependency crossing it is wrong today rather than merely worse than yesterday. The **health delta** compares the working tree against a revision and reports what a change worsened, without failing the build, because a threshold tight enough to be useful is too tight to be automatic.

### Acceptance-time integration

`aer_core::architecture_health` turns a verdict into a decision with a durable record.

Every acceptance journals its outcome, including the clean ones, because a series that records only bad news cannot show a trend and the repeated-hotspot trigger needs history it can count. A crossed boundary refuses acceptance outright. A single regression is recorded and allowed, because one regression is often the honest cost of a feature.

A refactoring task is created only when the same path and the same dimension have regressed three times. Two is a coincidence; three is a direction. The task identity is derived from the path and dimension, so a hotspot keeps pointing at one task instead of spawning a new one per regression, and the count is read from the journal so a restarted runtime counts what the previous one did.

## EvolutionBench

`tools/aer-bench` contains `aer-evolution-bench`, which replays one deterministic synthetic engineering trajectory under four regimes and reports what each did.

The measurement is the shipped controller — the harness renders real source text and calls the same measurement, duplication scan and evaluation the product uses, so a controller bug appears in the results. The engineer, however, is a rule and not a language model: it appends work to the largest unit it has already touched, and extracts a new unit when a gate objects. This therefore measures **the gate's effect on one modelled failure mode**, and is not evidence about real model trajectories. The receipt carries that limit in the artifact rather than only in prose.

Extraction is deliberately not free: each redirect adds a unit, adds lines, and leaves an identical call preamble behind, so gating can and does make duplication worse. No regime is declared a winner by the harness.

### Recorded result

120 tasks, 6 files, seed `0x5eed12349876abcd`. Receipt: `benchmarks/evolution/aer-evolution-bench-v1.json`.

| Regime | Redirects | Largest unit | Worst concentration | Total lines | Units | Duplicated lines |
|---|---|---|---|---|---|---|
| ungated (control) | 0 | 271 | 10000 bps | 1140 | 6 | 0 |
| default policy, previous-change baseline | 6 | 269 | 9607 bps | 1194 | 12 | 42 |
| default policy, twelve-task baseline | 69 | 235 | 6255 bps | 1761 | 75 | 483 |
| deliberately over-tight policy | 120 | 213 | 4122 bps | 2220 | 126 | 840 |

Two findings, one of them unflattering.

**The baseline matters more than the thresholds.** With a previous-change baseline the shipped default policy redirected six changes out of a hundred and twenty and moved the largest unit by two lines. It is close to inert against slow erosion, because erosion arrives nine lines at a time and nine is under every threshold. The *same policy* against a baseline twelve tasks back redirected sixty-nine changes and cut worst concentration by 37%. Tuning thresholds would not have found this; comparing baselines did.

**Structure is bought with duplication and size, monotonically.** Every regime that improved concentration paid for it. The strongest structural result also produced the most duplicated lines and nearly double the total lines. There is no regime here that improved everything.

### What this does and does not settle

It settles that health gating changes the trajectory, that the change is dominated by baseline distance rather than threshold choice, and that the improvement has a measurable cost. It does not settle anything about real model trajectories, and it must not be quoted as if it did.
