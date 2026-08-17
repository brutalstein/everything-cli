# Provider Context Economics Benchmark

## Status

This document defines the acceptance probe used to measure delegated-provider context economics. It is a measurement contract, not a provider optimization policy.

## Purpose

AER optimizes verified engineering outcome per unit cost. Provider prompt caching is one mechanism that may reduce marginal input cost, but cache telemetry must not be confused with engineering quality or treated as a score to game.

The benchmark exists to answer a narrower question:

> For byte-stable AER model context and a bounded canonical task, what fresh-input, cache-creation and cache-read behavior does the delegated provider actually report across repeated independent calls?

## Canonical probe

The benchmark is versioned as `provider-context-economics-v1`.

It compiles one exact AER `ModelContextEnvelope`, then dispatches the same provider-visible bytes through independent delegated CLI subprocesses. Claude Code continues to run with the production delegated-isolation flags; the benchmark does not relax permissions, settings isolation, MCP isolation or session persistence rules.

The canonical task asks the model to determine whether runtime permission mode may widen the capability ceiling and to emit exactly one sentinel:

- `AER_CACHE_PROBE_OK` when the answer is no;
- `AER_CACHE_PROBE_FAIL` otherwise.

The short fixed output is deliberate. Provider-reported total cost remains observable, but output-token variance is not allowed to masquerade as an input-cache improvement or regression.

## Required telemetry

Every sample preserves, without fabrication:

- fresh/uncached input tokens;
- cache-creation input tokens;
- cache-read input tokens;
- exact observed input total only when all three dimensions are known;
- fresh/cache-write/cache-read shares in integer basis points;
- output and reasoning tokens when reported;
- provider-reported cost when reported;
- resolved model identities;
- provider request/session identity when reported;
- wall-clock duration;
- exact model-context digest;
- canonical-output contract result.

Unknown provider fields remain unknown. Zero is never substituted for missing telemetry.

## Measurement integrity

A run set is measurement-valid only when all of the following hold:

1. at least two independent calls completed;
2. every output satisfies the canonical sentinel contract;
3. the model-context digest is identical across calls;
4. the resolved-model set is identical across calls;
5. all input dimensions required for an exact observed input total are present.

Cache efficiency is **not** part of measurement validity. A valid benchmark is allowed to reveal poor cache reuse.

## Aggregate statistics

The report keeps the first call separate from steady-state calls. For calls after the first it reports deterministic integer medians for:

- fresh input;
- cache creation;
- cache read;
- each input-class share.

It also reports:

- exact observed input min/max/median and spread;
- first-to-steady cache-read token delta;
- first-to-steady cache-creation token delta.

No synthetic "cache score", guessed token price, quality percentage or provider-neutral cost multiplier is permitted. Provider pricing changes independently and engineering quality must be evaluated by verification evidence, not by token ratios alone.

## CLI

Canonical usage:

```text
everything provider benchmark claude --runs 3 --json
```

`runs` is bounded to 2..10 so an accidental benchmark cannot fan out into unbounded paid model calls.

## Interpretation

For Anthropic telemetry, total observed input is the sum of fresh input, cache creation input and cache read input when all are reported. Anthropic documents cache reads and cache writes as separately priced dimensions; therefore they remain separate throughout AER telemetry.

A higher cache-read share can be economically useful, but it is not automatically better if engineering quality, latency, model choice or verification outcome degrades. The system-level objective remains verified engineering outcome per unit cost.

## Follow-on calibration

This benchmark should be run on the target machine after context-affecting changes. Its evidence is the input to later work on:

- provider-owned prefix/churn isolation;
- ContextSizer calibration;
- retrieval budget tuning;
- cache-aware routing policy.

Those policies must be derived from measured evidence. They must not hard-code aspirational cache percentages before data exists.
