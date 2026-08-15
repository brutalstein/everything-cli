# Observability and Cost Accounting

## Principle

An adaptive system that cannot explain its decisions cannot improve safely.

AER should emit OpenTelemetry-compatible traces, metrics, and structured events, using GenAI semantic conventions where appropriate plus AER-specific attributes.

## Trace hierarchy

```text
ProjectRun
  IntentSession
  Task
    Retrieval
    ContextPackBuild
    RoutingDecision
    ModelCall
      ToolCall*
    SandboxCommand*
    Verification*
    Integration*
```

## Required identifiers

Propagate:

```text
project_id
run_id
task_id
attempt_id
model_call_id
context_pack_id
repo_snapshot
spec_version
policy_versions
sandbox_id
```

## Model accounting

Record provider-reported values where available:

- input tokens,
- output tokens,
- reasoning tokens,
- cache read/write tokens,
- latency / time-to-first-token,
- request cost based on versioned pricing snapshot,
- retries/rate-limit errors.

Pricing MUST be timestamped because provider prices change.

## Tool accounting

Track:

- tool selection,
- duration,
- result size,
- context tokens produced from result,
- side-effect class,
- failures/retries.

## Context metrics

- candidate count,
- tokens selected,
- retrieval source mix,
- context yield,
- later missing-context requests,
- unused selected context,
- compression ratio,
- fidelity-check failures.

## Routing metrics

- selected model and reason features,
- candidate eligible models,
- predicted success/cost,
- actual verified outcome,
- escalation/de-escalation,
- offline regret when counterfactual labels exist.

## Orchestration metrics

- ready/running task counts,
- worker utilization,
- parallel speedup,
- merge-conflict rate,
- integration failure rate,
- coordination token overhead.

## Long-horizon metrics

- repeated read rate,
- repeated command rate,
- hypothesis churn,
- context resets,
- recovery events,
- rework after accepted tasks,
- architecture health trajectory.

## Privacy

OpenTelemetry allows prompt/content capture, but AER MUST make full-content telemetry opt-in and policy-controlled.

Default telemetry should prefer:

- hashes,
- IDs,
- sizes,
- derived metrics,
- redacted excerpts only when needed.

## User-facing inspection

CLI should support commands such as:

```text
aer inspect run <id>
aer inspect cost <id>
aer inspect context <task>
aer inspect route <task>
aer inspect proof <task>
```

Every adaptive decision should be inspectable without exposing hidden model reasoning.
