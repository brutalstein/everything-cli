# Core Invariants and Property Tests

Many AER correctness properties belong in deterministic/property tests rather than model evals.

## State invariants

1. A task has at most one non-expired active execution lease.
2. `accepted` task state requires an accepted verifier verdict.
3. Accepted verifier verdict requires required evidence policy to be satisfied.
4. Evidence binds to an exact repository snapshot.
5. A task binds to an Engineering IR version.
6. A stale task cannot be newly integrated without replanning.
7. A disproven hypothesis cannot be emitted as a verified fact without new superseding evidence.
8. Security policy cannot be widened by a lower-authority project/run setting.
9. A model/worker cannot directly mutate authoritative project state.
10. Every adaptive decision used in a run references immutable policy version IDs.

## Event invariants

- event IDs are unique;
- causation references point to existing or explicitly external events;
- replay produces equivalent critical state projections;
- large artifact hashes resolve or are explicitly marked expired by retention policy;
- task acceptance is never observed before corresponding verifier/evidence journal entries.

## Budget invariants

- hard organization budget cannot be exceeded by task-level override;
- parallel child budgets cannot exceed parent/global hard cap unless policy records approved expansion;
- cost accounting never assumes unknown provider tokens are exact: estimated values are labeled estimated.

## Sandbox invariants

For sandbox conformance tests:

- workspace writes succeed where allowed;
- writes outside workspace fail;
- disallowed network egress fails;
- host credentials are not readable;
- host Docker/control socket is unavailable by default;
- verifier immutable mount is not writable;
- subprocess restrictions propagate to descendants.

## Repository/context invariants

- Context Pack source refs resolve against its declared repo snapshot;
- content hash mismatch invalidates a cached context item;
- changed source invalidates affected index entries;
- retrieval may return zero results without forced hallucinated context.

## Verification invariants

- generator modification of visible test files is detected and surfaced;
- held-out verifier identity is immutable for the run;
- cached evidence is invalidated when dependency fingerprints change;
- project completion requires all `must` requirements to have current accepted proof or explicit waiver.

## Property-testing targets

Use generative/property tests for:

- task state transition sequences,
- daemon crash/restart at each lifecycle transition,
- concurrent lease acquisition,
- SpecDelta invalidation,
- event replay/projection equivalence,
- context budget never exceeding hard cap,
- policy precedence lattice.

## Model eval vs property test rule

If a correctness property can be expressed deterministically, test it deterministically. Reserve model-based evals for semantic judgments that cannot yet be made robustly executable.
