# Core Invariants and Property Tests

Many AER correctness properties belong in deterministic/property tests rather than model evals.

## State invariants

1. A task has at most one non-expired active execution lease.
2. `accepted` task state requires an accepted verifier verdict.
3. Accepted verifier verdict requires required evidence policy to be satisfied.
4. Evidence binds to an exact repository snapshot and relevant environment identity.
5. A task binds to an Engineering IR version.
6. A stale task cannot be newly integrated without replanning.
7. A disproven hypothesis cannot become a verified fact without new superseding evidence.
8. Security policy cannot be widened by a lower-authority project/run setting.
9. A model/worker cannot directly mutate authoritative project state.
10. Every adaptive decision used in a run references immutable policy version IDs.
11. Research claims cannot silently become user decisions/requirements.
12. An incompatible durable-state version cannot enter write mode before successful migration/preflight.

## Event invariants

- event IDs are unique;
- causation references point to existing or explicitly external events;
- replay produces equivalent critical state projections;
- large artifact hashes resolve or are explicitly marked expired by retention policy;
- task acceptance is never observed before corresponding verifier/evidence journal entries;
- historical events are never silently reinterpreted under a new schema without version-aware decoding/upcast rules.

## Budget and resource invariants

- hard organization budget cannot be exceeded by task-level override;
- parallel child budgets cannot exceed parent/global hard cap unless policy records approved expansion;
- cost accounting labels estimates as estimates;
- admitted worker/provider/tool concurrency never exceeds hard capacity;
- every internal queue/channel is bounded or backed by an explicitly bounded spill mechanism;
- durable authoritative events are never silently dropped due to queue pressure;
- verification can reserve capacity and cannot be permanently starved by generators;
- cancellation eventually releases or reconciles owned resource reservations.

## Provider invariants

- retries are bounded and classified;
- a retry never silently duplicates a non-idempotent external effect;
- partial streams never become completed structured results without validation;
- failover re-runs eligibility/privacy checks for the replacement endpoint;
- rate-limit/circuit state affects admission rather than creating retry storms;
- provider/model/pricing identity used for a run is inspectable.

## Sandbox invariants

For sandbox conformance tests:

- workspace writes succeed where allowed;
- writes outside workspace fail;
- disallowed network egress fails;
- host credentials are not readable;
- host Docker/control socket is unavailable by default;
- verifier immutable mount is not writable;
- subprocess restrictions propagate to descendants.

## Workspace/VCS invariants

- user-owned dirty changes are never reset, discarded, or silently stashed;
- AER writable execution occurs in owned isolated workspaces unless policy explicitly says otherwise;
- remote push/PR/publication requires its declared capability;
- an upstream-base change invalidates integration assumptions that depend on the old base;
- cleanup never deletes unintegrated user-owned work.

## Repository/context invariants

- Context Pack source refs resolve against its declared repo snapshot;
- content hash mismatch invalidates a cached context item;
- changed source invalidates affected index entries;
- retrieval may return zero results without forced hallucinated context;
- decision-critical compressed context retains source provenance.

## Research invariants

- every promoted research claim resolves to source refs and observation time;
- source text cannot grant execution authority;
- contested high-impact claims cannot be flattened into verified fact without resolution policy;
- freshness-expired claims are not reused as current facts without refresh or explicit waiver;
- research artifacts preserve contradictory evidence needed for audit.

## Environment and dependency invariants

- evidence-cache reuse requires matching declared dependency/environment fingerprints;
- lockfile/toolchain changes invalidate affected evidence;
- dependency installs obey network/security policy;
- executable package hooks run with sandbox authority, never implicit host authority;
- release/build provenance cannot claim a stronger reproducibility level than observed.

## Verification invariants

- generator modification of visible test files is detected and surfaced;
- held-out verifier identity is immutable for the run;
- cached evidence is invalidated when dependency fingerprints change;
- project completion requires all `must` requirements to have current accepted proof or explicit waiver;
- domain profile selection cannot weaken mandatory project/org verification gates.

## Compatibility/release invariants

- unsupported schema/API version combinations fail before destructive mutation;
- migration either completes with postconditions or leaves explicit recoverable state;
- downgrade is refused when the older binary cannot safely read current state;
- updater rejects artifacts/metadata that fail trust, freshness or anti-rollback policy;
- self-evolution policy artifacts cannot bypass ordinary trusted release/promotion boundaries.

## Data-governance invariants

- secret-class data is never written to ordinary telemetry/artifact stores;
- retention deletion removes or tombstones derived artifacts according to dependency policy;
- cross-project learning never copies project source/prompt content in `aggregate_only` mode;
- tenant/project scope is preserved across indexes, caches, events and exported telemetry.

## Property-testing targets

Use generative/property tests for:

- task state transition sequences;
- daemon crash/restart at each lifecycle transition;
- concurrent lease/resource acquisition;
- bounded queue pressure;
- provider retry/failover/cancellation sequences;
- SpecDelta invalidation;
- event replay/projection equivalence;
- context budget never exceeding hard cap;
- policy precedence lattice;
- migration crash points and compatibility handshakes;
- dirty-worktree/upstream-drift sequences;
- retention/invalidation cascades;
- arbitrary Unicode/terminal resize in UI projection.

## Model eval vs property test rule

If a correctness property can be expressed deterministically, test it deterministically. Reserve model-based evals for semantic judgments that cannot yet be made robustly executable.
