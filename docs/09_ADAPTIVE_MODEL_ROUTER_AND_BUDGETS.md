# Adaptive Model Router and Budget Controller

## Objective

Select the model, effort level, context budget, and escalation strategy that maximize expected verified outcome under cost/latency/risk constraints.

Routing is a sequential decision problem, not merely classification of the initial prompt.

## Initial production policy

V1 SHOULD be deterministic and interpretable:

1. filter endpoints by security/data/capability constraints;
2. estimate task complexity and risk;
3. select the cheapest model whose confidence exceeds the policy threshold;
4. use a scout/exploration stage for uncertain repository tasks;
5. escalate on evidence of difficulty or failure.

Do not deploy an online-learning router before sufficient telemetry exists.

## Two-stage scout routing

For many repository tasks, the initial user issue does not contain enough information for optimal model selection.

A cheap scout MAY perform bounded exploration:

- locate relevant subsystem,
- identify language/build system,
- reproduce failure,
- estimate likely change surface,
- emit a structured, evidence-backed scouting report.

The router then decides whether to continue with the scout-tier model or escalate.

This follows the useful principle from recent temporal routing research: partial execution trajectory can carry routing information unavailable in task text alone.

## Router feature vector

```text
TaskFeatures {
  task_kind
  languages
  repo_scale
  expected_change_surface
  ambiguity
  risk
  context_requirement
  tool_requirement
  current_failure_count
  previous_model_results
  architecture_sensitivity
  latency_sensitivity
  user_quality_mode
}
```

## Utility objective

Do not optimize cost alone.

Conceptual objective:

```text
U(model, action | state) =
  E[verified_success]
  - λc * expected_cost
  - λt * expected_latency
  - λr * regression_risk
  - λa * architecture_damage_risk
```

Weights derive from project/user policy. For a critical migration, success/risk dominates cost. For bulk low-risk cleanup, cost dominates more strongly.

## Escalation triggers

Escalate model capability or reasoning effort when:

- repeated test failures provide new but unresolved evidence;
- confidence remains low after scouting;
- task spans architectural boundaries;
- security/critical correctness risk is high;
- loop/stagnation detector triggers;
- verifier rejects a plausible implementation for a nontrivial reason;
- context retrieval indicates unusually large dependency surface.

Do not escalate merely because a fixed number of tokens were consumed.

## De-escalation

Cheap models SHOULD handle deterministic-adjacent work such as:

- summarizing already-verified logs,
- classification,
- formatting structured artifacts,
- low-risk repetitive edits if verified by the same gates.

## Budget controller

Each task gets separate budgets:

```text
input_tokens
output_tokens
cached_tokens
tool_calls
wall_time
model_cost
parallel_workers
network_bytes (optional)
```

Budgets are soft until risk policy says hard.

A model may request more budget with justification. The controller decides based on marginal expected value.

## Learned routing later

Once sufficient execution-grounded history exists, introduce a constrained contextual-bandit / streaming-regret router.

Requirements:

- offline replay evaluation first;
- safe exploration bounds;
- no routing to security-ineligible endpoints;
- explicit confidence intervals;
- deterministic fallback policy;
- policy version in every run.

Candidate algorithms may include Thompson sampling, UCB variants, or learned value models. The architecture intentionally does not freeze one algorithm before data exists.

## Router regret

For benchmark/replay tasks where multiple model outcomes are known, measure cumulative regret relative to the best eligible model for each task.

For live tasks, use counterfactual estimates carefully; do not pretend unknown alternative outcomes are observed facts.

## User modes

Expose simple user-facing policies:

- `economy`
- `balanced` (default)
- `maximum-quality`

These alter utility weights and escalation thresholds, not hard-coded model names.
<!-- context-economy-v2-cognitive-routing -->
## Context Economy V2: cognitive work roles and deterministic-first routing

The routing layer now has provider-neutral cognitive work roles: `Deterministic`, `Scout`, `Locator`, `RepositoryExplorer`, `FailureAnalyst`, `EvidenceCompressor`, `Planner`, `Coder`, and `SemanticReviewer`. These are work capabilities, not model names. They do not grant execution authority and they do not require every role to invoke an LLM.

The default policy is deterministic-first: exact retrieval, graph traversal, freshness/state tracking, validation, context assembly, and cheap classification remain in AER's deterministic control plane whenever adequate. Bounded ambiguous scouting/localization may later be assigned to a low-cost or local provider. Planning/coding/semantic review must not be silently downgraded solely for price when the requested reasoning capability is not met. The deterministic verifier remains the acceptance authority.

Provider/model eligibility continues to be resolved from capabilities and policy. Cache geometry is also a transport capability, not a model-name branch. An OpenAI-compatible multi-provider gateway can therefore be added later as transport while AER retains requested capability, task role, security constraints, budgets, escalation, verification, and acceptance. An external router's `auto` policy is not AER's control plane.
