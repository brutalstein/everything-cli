# Configuration and Policy Model

## Goal

Expose simple intent-level configuration to users while keeping detailed decision policy versioned, bounded and inspectable.

## Precedence

Recommended precedence from highest to lowest for ordinary preferences:

1. explicit current CLI flags / run overrides,
2. project-local `.aer/config.yaml`,
3. user config,
4. organization defaults,
5. built-in defaults.

Security, data, resource and release trust are lattices rather than ordinary overwrite settings: lower-trust layers MAY restrict but MUST NOT widen organization-imposed authority/retention/provider limits.

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
  retention_profile: standard

ui:
  mode: auto
  motion: reduced
  unicode: true
  color: auto
  notifications: important

updates:
  channel: stable
  check: auto
```

The executable baseline is `schemas/config.schema.json`.

## Policy vs configuration

Configuration expresses constraints/preferences.

Policy decides behavior within them.

Example:

```text
config: max_run_cost = $30
policy: scout cheaply, then escalate only if expected verified value justifies it
```

## Versioned policy families

- intent-question policy;
- research/source policy;
- task decomposition policy;
- context scoring/selection policy;
- model routing policy;
- provider retry/health policy;
- budget/resource admission policy;
- parallelism policy;
- verification/domain-profile composition policy;
- recovery policy;
- architecture-health policy;
- skill routing policy;
- retention/data-governance policy where organization-controlled.

Each run records exact material policy IDs.

## Configuration validation

Unknown keys should fail or warn according to strictness mode. Security/data/release-sensitive keys MUST NOT be silently ignored.

Normative documentation examples and config schema must be tested together so the docs cannot advertise rejected keys.

## Secrets

Provider keys and credentials MUST NOT live in ordinary project config.

Resolve through OS keychain, environment secret provider or credential broker.

A config file may reference a logical credential name but not embed secret material as the normal path.

## Resource limits

Organization/global hard limits cannot be widened by project config.

A value such as:

```yaml
parallelism:
  max_workers: 64
```

is a request bounded by Resource Governor policy, not authority to create 64 workers.

## Data and retention

Privacy settings select among policy-permitted profiles. They do not override legal/organization minimum/maximum retention or provider eligibility.

See `42_DATA_GOVERNANCE_RETENTION_AND_TENANCY.md`.

## Update configuration

Update channel/check settings control discovery/UX only. They cannot disable signature/freshness/compatibility verification for an update that is applied.

See `40_VERSIONING_MIGRATIONS_AND_RELEASE_SAFETY.md`.

## Reproducible modes

Eval/replay runs may pin:

- provider/model snapshot;
- all material policy versions;
- context budgets;
- sandbox image;
- toolchain/dependency/environment fingerprints;
- research source snapshots where feasible;
- domain verification profile versions.

Normal interactive use may select latest eligible resources according to policy and capability registry, while recording what was actually used.
