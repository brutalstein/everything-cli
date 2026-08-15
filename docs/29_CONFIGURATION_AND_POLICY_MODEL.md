# Configuration and Policy Model

## Goal

Expose simple intent-level configuration to users while keeping detailed policy versioned and inspectable.

## Precedence

Recommended precedence from highest to lowest:

1. explicit current CLI flags / run overrides,
2. project-local `.aer/config.yaml`,
3. user config,
4. organization policy,
5. built-in defaults.

Security policy is special: lower-trust layers may restrict but MUST NOT widen organization-imposed capability limits.

## User-facing configuration

Example:

```yaml
quality_mode: balanced
security_profile: sandboxed
autonomy: workspace

models:
  allowed_providers: [openai, anthropic, google]
  max_run_cost_usd: 30

context:
  max_tokens_per_call: 32000

parallelism:
  max_workers: 4

privacy:
  store_prompt_content: false
  cross_project_learning: aggregate_only
```

## Policy vs configuration

Configuration expresses constraints/preferences.

Policy decides behavior within those constraints.

For example:

```text
config: max_run_cost = $30
policy: choose scout model, then escalate if uncertainty remains > threshold
```

## Versioned policy families

- intent-question policy,
- task decomposition policy,
- context scoring/selection policy,
- model routing policy,
- budget policy,
- parallelism policy,
- verification composition policy,
- recovery policy,
- architecture-health policy,
- skill routing policy.

Each run records exact policy IDs.

## Configuration validation

Unknown keys should fail or warn according to strictness mode. Never silently ignore a security-sensitive setting.

## Secrets

Provider keys and credentials MUST NOT live in ordinary project config. Resolve through OS keychain, environment secret provider, or credential broker.

## Reproducible modes

Eval runs may pin:

- provider/model snapshot,
- all policy versions,
- context budgets,
- sandbox image,
- tool versions.

Normal interactive use may allow latest eligible models according to capability registry.
