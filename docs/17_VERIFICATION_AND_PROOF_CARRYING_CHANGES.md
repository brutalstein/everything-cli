# Verification and Proof-Carrying Changes

## Thesis

Verification is a first-class subsystem and may be harder than generation. AER must not optimize only for visible test passing.

## Acceptance model

A change is accepted when required evidence satisfies:

1. user/Engineering IR intent,
2. functional correctness,
3. regression constraints,
4. security/quality requirements,
5. architecture-health policy,
6. verifier integrity.

## Verification layers

### V0 — mechanical

- parse/compile,
- formatter consistency,
- typecheck,
- lint where meaningful.

### V1 — local behavior

- unit tests,
- focused reproduction,
- component tests.

### V2 — integration

- integration/contract tests,
- migration tests,
- end-to-end workflows.

### V3 — non-functional

- performance,
- security scanners,
- resource limits,
- compatibility,
- accessibility where relevant.

### V4 — architecture and maintainability

- dependency boundary checks,
- complexity/duplication deltas,
- API surface changes,
- structural erosion signals.

### V5 — semantic/independent review

- requirement-to-diff review,
- independent model verifier when policy requires,
- adversarial/held-out tests for high-risk tasks.

## Verifier composition

No one verifier is assumed sufficient. Verification policy selects layers based on task risk/type.

Example:

```text
small internal refactor:
  V0 + V1 + architecture delta

public authentication change:
  V0 + V1 + V2 + security + independent semantic review + hidden cases
```

## Verifier independence

Generator-controlled files cannot be the sole oracle.

Where feasible:

- verifier definitions are mounted read-only;
- hidden cases are unavailable during generation;
- test infrastructure hashes are recorded;
- generator cannot alter acceptance thresholds;
- verifier runs in a clean environment.

## Reward-hacking defenses

Detect suspicious changes such as:

- deleting/skipping tests,
- broad exception swallowing,
- hard-coded fixture answers,
- environment-specific bypasses,
- weakening assertions,
- modifying benchmark/verifier data,
- changing config to avoid execution,
- fake success output.

A visible-test pass + held-out fail is an integrity signal, not just another bug.

## Evidence record

Each evidence item includes:

```text
evidence_id
type
requirement_refs[]
repo_snapshot
command/tool identity
environment_fingerprint
input artifact hashes
output artifact hashes
exit/result
measurements
timestamp
integrity metadata
```

See `schemas/evidence.schema.json`.

## Proof Manifest

Every accepted task emits a proof fragment mapping semantics to implementation and evidence.

```yaml
requirement: REQ-WS-004
implementation:
  - path: src/ws/session.rs
    symbol: Session::resume
verification:
  - evidence: EVID-001
  - evidence: EVID-002
verdict: pass
```

Project-level proof is the union of accepted fragments plus current regression evidence.

## Independent semantic verifier

For medium/high risk, a verifier model receives:

- requirement/acceptance criteria,
- diff or affected code,
- deterministic evidence,
- architecture constraints.

It SHOULD NOT receive the generator's persuasive self-assessment as primary evidence.

## Backward reconstruction check

Optionally ask a verifier to infer what behavior the patch implements from code/evidence, then compare that reconstruction with the intended requirement. Large mismatch is a useful semantic alarm.

## Verification caching

Evidence can be reused only when its dependency fingerprint remains valid. File hashes, build config, dependency lockfiles, environment and relevant inputs determine invalidation.

## Completion statement

The runtime should report:

```text
17/17 required requirements have accepted proof
1 non-blocking quality warning
all critical verification gates pass
```

not merely “done.”
