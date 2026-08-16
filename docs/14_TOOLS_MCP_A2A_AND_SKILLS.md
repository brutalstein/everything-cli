# Tools, MCP, A2A, and Skills

## Internal rule

AER owns an internal **Tool ABI** optimized for correctness and policy enforcement. External protocols are adapters.

This prevents every local compiler/git/file operation from paying protocol and schema overhead while preserving ecosystem interoperability.

## Tool descriptor

Each tool exposes:

```text
id
version
input_schema
output_schema
side_effect_class
required_capabilities
timeout_policy
idempotency
cost_hint
security_labels
```

Tool results are structured and may reference large artifacts by hash rather than injecting them into context.

## Dynamic discovery

Do not preload hundreds of tool schemas into every prompt.

Models should receive:

1. small core tool set,
2. searchable catalog metadata,
3. detailed schema only when selected.

This follows progressive disclosure and reduces context/tool-selection noise.

## MCP

AER SHOULD implement an MCP adapter compatible with the current `2026-07-28` specification.

Use MCP for external tools/resources that benefit from standardized discovery and authorization.

Important design points from the current spec:

- stateless core;
- official Tasks extension for long-running operations;
- per-request authorization remains necessary;
- state handles are not authentication capabilities;
- OAuth 2.1-oriented authorization practices.

AER MUST enforce its own side-effect/security labels even when an MCP server advertises a tool.

## A2A

A2A v1.0 is useful at the boundary between independent/opaque agent systems.

AER SHOULD support an optional A2A gateway for:

- remote specialist organizations,
- enterprise agent interoperability,
- delegation to independently managed agents.

AER SHOULD NOT use A2A as its internal hot-path handoff representation. Internal Handoff ABI is more specific, lower overhead, and deeply tied to Engineering IR/evidence.

## Skills

A skill is a versioned procedural knowledge artifact, not an always-on persona.

Skill metadata:

```text
skill_id
version
applicability
required_tools
supported_versions
token_cost_estimate
security_level
source/provenance
eval_results
historical_utility
```

## Skill activation

A Skill Router chooses skills based on relevance and observed utility.

Do not activate a skill merely because its description has semantic similarity. Consider:

- exact framework/version compatibility,
- task state,
- past success,
- expected token/tool overhead,
- conflict with project rules.

## Skill lifecycle

```text
candidate -> evaluated -> approved -> active -> deprecated/quarantined
```

Model-generated skills enter `candidate` state only.

## Tool/skill security

Treat tool descriptions, remote resources, fetched docs, repository comments, and external agent output as untrusted content. They may contain prompt injection.

Authority derives from the control plane and capability policy, not from text encountered by a model.

## Implemented native ToolBroker baseline

The first native Tool ABI hot path is implemented in `aer-core::tools` and follows the authority rules in `45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md`.

The implemented baseline is deliberately small: `fs.read`, `fs.list`, structured `exec.run`, `tool.search`, and `tool.describe`. Reads/listings/command output are bounded; `exec.run` produces argv/cwd/exit/timeout/output-hash evidence; tool schemas use progressive disclosure. MCP/A2A/provider-native tools remain adapters around this internal authority, not competing execution paths.

The first delegated provider smoke keeps provider-native tools disabled. A later structured protocol bridge may submit tool proposals to the ToolBroker, but provider output cannot execute host actions directly.
