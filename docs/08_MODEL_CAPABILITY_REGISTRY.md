# Model Capability Registry

## Purpose

Maintain an empirical, time-sensitive description of available intelligence resources.

The registry distinguishes **provider claims** from **AER-observed performance**.

## Model identity

A model endpoint is identified by:

```text
provider
model_family
model_id
version_or_snapshot
endpoint_class
region (optional)
```

Never assume a marketing alias is immutable.

## Capability dimensions

Store declared and observed capabilities such as:

- natural-language intent elicitation,
- architecture reasoning,
- repository coding,
- debugging,
- tool-use reliability,
- structured-output adherence,
- long-context behavior,
- multimodal input,
- latency,
- reasoning/effort controls,
- maximum context/output,
- cache support,
- cost,
- rate limits,
- sandbox/tool compatibility.

## Empirical performance

AER MUST gradually build task-conditioned statistics.

Example:

```text
(model, task_features) -> {
  attempts,
  verified_success_rate,
  mean_cost,
  p50/p95 latency,
  mean tool calls,
  regression rate,
  architecture-health delta,
  confidence interval
}
```

Task features may include:

- language,
- task category,
- change size,
- repository size,
- ambiguity,
- debugging vs greenfield,
- UI vs systems,
- context size,
- verifier type.

## Cold start

Do not invent precision for a new model.

Use:

1. provider-declared capabilities,
2. curated bootstrap evals,
3. conservative priors from model family,
4. rapid shadow evaluation on representative tasks.

Production routing SHOULD reflect uncertainty until sufficient evidence exists.

## Freshness and drift

Model behavior changes with snapshots and service updates.

Capability records require:

- observation window,
- sample count,
- endpoint fingerprint/version when exposed,
- decay or staleness policy.

Historic results remain auditable but should not dominate routing indefinitely.

## Provider adapter contract

Every adapter exposes normalized operations:

```text
Generate(request) -> stream/result
CountOrEstimateTokens(...)
Capabilities()
PricingSnapshot()
Health()
```

Requests include:

- messages / structured context,
- tools,
- output schema,
- effort/reasoning mode if supported,
- max output,
- stop/cancel handle,
- cache hints where supported.

Provider-specific features may be exposed as optional capability flags, not leaked throughout the core.

## Privacy classification

Endpoints also record policy constraints:

- allowed data sensitivity,
- retention mode,
- region requirements,
- enterprise endpoint eligibility.

The router must filter disallowed models before optimizing quality/cost.
