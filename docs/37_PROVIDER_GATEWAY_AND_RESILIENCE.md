# Provider Gateway and Resilience

## 1. Objective

Normalize model-provider execution without pretending providers have identical behavior.

The Provider Gateway owns operational semantics for:

- capability negotiation,
- request identity,
- streaming assembly,
- structured output,
- cancellation,
- rate limits,
- retries,
- health,
- circuit breaking,
- fallback,
- usage/cost normalization,
- provider-specific error translation.

Routing decides **which eligible intelligence resource should be used**. The Provider Gateway makes that decision execute safely.

## 2. Provider operation identity

Every model attempt receives immutable AER identity:

```text
model_call_id
attempt_id
provider
endpoint
model_id
model_snapshot_if_known
request_fingerprint
context_pack_id
tool_catalog_version
policy_versions
```

Provider request IDs SHOULD be captured when exposed.

A retry is a new attempt linked to the same logical model call. It is never silently overwritten.

## 3. Capability handshake

Adapters expose runtime capabilities, not static assumptions:

```text
max_context
max_output
structured_output
tool_calls
parallel_tool_calls
streaming
multimodal_inputs
prompt_cache
reasoning_controls
cancellation
batch
data_residency
retention_class
```

Capability Registry records declared and empirically observed behavior.

If a required capability disappears or drifts, routing eligibility changes before execution.

## 4. Normalized error taxonomy

Provider-specific errors map to typed categories such as:

```text
invalid_request
authentication
authorization
content_policy
rate_limited
quota_exhausted
transient_unavailable
provider_internal
timeout
connection
stream_interrupted
schema_violation
context_overflow
cancelled
unknown
```

Raw provider error/request IDs remain attached for diagnosis.

## 5. Retry policy

Retry only when evidence suggests the operation is safely repeatable.

Initial policy:

- transient connection/provider failures: bounded exponential backoff with jitter;
- rate limits: honor provider reset/retry hints where available;
- invalid/auth/policy/context errors: do not blind-retry;
- schema violation: one bounded repair/re-render path MAY occur if policy allows;
- repeated transient failures: circuit breaker opens.

Retries consume budget and telemetry.

## 6. Tool-call deduplication

A retried/continued model stream MUST NOT cause external tool side effects to execute twice.

Every proposed tool call receives an AER idempotency/dedup identity before dispatch.

For non-idempotent tools:

- duplicate identity returns prior result when safe;
- ambiguous execution state becomes `inconclusive/external_state_unknown`;
- the controller verifies external state before retry.

## 7. Streaming state machine

Streaming is assembled into typed events:

```text
started
content_delta*
tool_call_delta*
usage_delta*
completed | failed | cancelled | interrupted
```

Partial JSON/tool arguments are not executable until syntactically complete and schema-valid.

If a stream dies after tool execution but before final model completion, evidence/tool state is preserved; the recovery path does not pretend the whole attempt never happened.

## 8. Structured output

Provider-native schema features MAY be used, but AER performs its own validation.

Pipeline:

```text
provider output
 -> parse
 -> JSON/schema validation
 -> semantic validation
 -> accepted WorkResult candidate
```

Malformed output is data, not authoritative state.

## 9. Rate-limit governor integration

Adapters surface current limits when available:

- requests/time,
- tokens/time,
- concurrent requests,
- daily/monthly quota,
- reset hints.

The Resource Governor reserves provider capacity before dispatch when possible.

Do not discover rate limits only by creating a storm of rejected requests.

## 10. Circuit breakers and health

Endpoint health states:

```text
healthy
degraded
rate_limited
open_circuit
unavailable
disabled_by_policy
```

Health is scoped to endpoint/model/region where possible; one failing model must not mark an entire provider dead.

## 11. Fallback semantics

Provider failover is not transparent string replacement.

Fallback is allowed only when the candidate:

- satisfies data/privacy policy,
- satisfies required capabilities,
- fits remaining budget,
- does not violate model pinning/reproducibility,
- receives a fresh cognitive-adapter rendering of the same semantic handoff.

The event journal records the switch and reason.

For evaluation/reproducible runs, silent fallback is forbidden unless explicitly part of the pinned policy.

## 12. Provider drift

The system periodically rechecks:

- model existence/alias movement,
- context/output limits,
- tool/structured-output behavior,
- pricing,
- retention/privacy policy,
- latency/reliability,
- empirical task quality.

Marketing aliases MUST NOT be treated as immutable snapshots.

## 13. Concurrency and cost

A request is admitted only after:

- global/run/task budget check,
- provider quota reservation,
- cancellation token registration.

Usage accounting distinguishes provider-reported, estimated, and reconciled values.

## 14. Test requirements

Provider adapters require:

- contract tests with recorded fixtures,
- stream fragmentation tests,
- malformed tool/schema tests,
- retry/idempotency tests,
- rate-limit simulations,
- cancellation races,
- partial-stream recovery,
- provider drift fixtures,
- failover policy tests.

Core correctness tests MUST NOT require live paid APIs.
