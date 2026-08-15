# Domain Capability and Verification Profiles

## 1. Objective

AER is a general software-engineering runtime. “Run tests” means different things for a web product, CLI, database migration, mobile app, ML pipeline, systems component, or infrastructure repository.

Core verification remains generic; **Domain Profiles** declare domain-specific tools, environments, evidence, and acceptance defaults.

A Domain Profile is not a fixed agent persona.

## 2. Profile descriptor

A versioned profile may declare:

```text
profile_id
version
applicability
required/optional tools
environment requirements
default verifier composition
evidence types
health metrics
reference performance profiles
security/data constraints
known unsupported cases
eval results
```

Profile selection is task/project conditioned and inspectable.

## 3. Web/UI profile

Possible evidence:

- build/type/lint,
- browser end-to-end behavior,
- accessibility audit,
- responsive viewport checks,
- console/network errors,
- performance measurements,
- screenshot/visual regression,
- keyboard navigation.

Visual evidence is stored as immutable artifacts tied to browser/viewport/theme/font/environment identity.

A multimodal model MAY review aesthetics/semantic visual defects, but subjective model taste cannot be the sole correctness oracle.

## 4. Backend/service profile

Possible evidence:

- unit/integration/contract tests,
- database migration round trips,
- API compatibility,
- concurrency/race tests,
- load/performance reference profile,
- fault/retry behavior,
- security checks.

## 5. CLI/TUI profile

Use `23_CLI_AND_USER_EXPERIENCE.md`:

- PTY tests,
- renderer snapshots,
- headless JSON contract,
- resize/crash restoration,
- latency/idle CPU,
- accessibility/plain mode.

## 6. Systems/native profile

Possible evidence:

- compiler warnings,
- sanitizers,
- race detectors,
- fuzz/property tests,
- architecture/ABI compatibility,
- resource bounds,
- platform matrices.

## 7. Data/ML profile

Possible evidence:

- data contracts/schema checks,
- deterministic preprocessing where possible,
- train/eval split integrity,
- seed/environment capture,
- metric confidence intervals,
- leakage tests,
- model artifact provenance,
- inference performance,
- drift/reproducibility checks.

A higher benchmark metric is not automatically acceptance if evaluation integrity is weak.

## 8. Infrastructure/IaC profile

Default to plan/dry-run.

Possible evidence:

- syntax/static analysis,
- policy-as-code,
- plan diff,
- least-privilege checks,
- ephemeral integration environment.

Production apply is a separate high-authority capability and is not implied by successful plan verification.

## 9. Mobile profile

Possible evidence:

- build/test on target SDK,
- simulator/emulator interaction,
- accessibility,
- layout across representative devices,
- permission/privacy behavior,
- startup/performance profiles.

## 10. Profile composition

A project may combine profiles.

Example:

```text
TypeScript web app
 = web-ui
 + backend-service
 + database
 + supply-chain
```

The Verification Controller unions required gates and resolves conflicts by stronger policy.

## 11. Capability discovery

Repository Intelligence MAY propose profile candidates based on:

- languages,
- build files,
- frameworks,
- deployment manifests,
- Engineering IR.

Final profile activation is deterministic/policy-validated.

## 12. Profile lifecycle

```text
candidate -> evaluated -> approved -> active -> deprecated
```

Model-generated profiles are candidates only.

## 13. Eval requirement

A profile must have domain fixtures demonstrating that it catches failures better than a generic test-only policy without unacceptable false positives/cost.

## 14. Extensibility boundary

Domain Profiles configure existing Tool/Sandbox/Verification ABIs.

Do not fork the core state machine for each domain.
