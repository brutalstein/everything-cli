# Data Governance, Retention, and Tenancy

## 1. Objective

AER stores source-derived context, model traffic metadata, tool output, traces, evidence, screenshots, research material, and long-horizon engineering state.

These artifacts need explicit data classification and lifecycle. “Local-first” reduces exposure; it does not remove governance.

## 2. Data classes

Baseline labels:

```text
public
internal
confidential
restricted
secret
```

Projects/organizations MAY refine these labels.

Every artifact/context/source SHOULD inherit the most restrictive applicable label unless an explicit declassification rule applies.

## 3. Policy dimensions

Data policy controls:

- model/provider eligibility,
- region/residency,
- network destinations,
- telemetry content capture,
- cross-project learning,
- retention duration,
- export,
- encryption requirements,
- remote-worker eligibility,
- deletion/garbage collection.

## 4. Secrets

`secret` data is not ordinary context.

Credentials SHOULD flow through brokers/capabilities rather than prompts or content-addressed artifacts.

If a secret is accidentally captured:

- mark the artifact contaminated,
- stop propagation,
- rotate/revoke where applicable,
- redact future views,
- preserve only the minimum audit record permitted by policy.

Hashes of low-entropy secrets can still leak information; do not assume hashing automatically makes sensitive values safe.

## 5. Retention

Object metadata includes:

```text
sensitivity
created_at
retention_class
expires_at
legal_hold/pin if supported
source_project
derived_from[]
```

Garbage collection must preserve referential integrity.

When an evidence artifact expires by policy, state records that it is unavailable/expired; the system must not pretend the proof is still fully inspectable.

## 6. Deletion

Deletion has two layers:

1. logical deletion / unreachable state,
2. physical object cleanup when policy permits.

AER should be transparent that perfect secure erasure may not be guaranteed on all filesystems/SSDs/backups.

## 7. Cross-project learning

Default:

```text
project content -> project local
global learning -> aggregate statistics only
```

Raw proprietary code, prompts, research documents, and evidence MUST NOT enter global reusable memory automatically.

Explicit content reuse requires policy and provenance.

## 8. Tenancy

The local v0.x runtime may be single-user, but durable records SHOULD carry project/tenant ownership fields so later remote execution cannot accidentally mix namespaces.

Future multi-tenant services require:

- authenticated principal,
- authorization checks at every object boundary,
- tenant-scoped encryption/storage where appropriate,
- audit trails,
- isolation tests.

Do not implement a cloud tenancy control plane prematurely; preserve the contract boundary.

## 9. Provider data controls

Provider eligibility is based on current policy facts such as:

- allowed sensitivity,
- retention behavior,
- training/data-use policy,
- region,
- enterprise endpoint guarantees.

These facts are time-sensitive Capability Registry inputs.

A provider becoming cheaper does not override data policy.

## 10. Telemetry

Full prompt/source/tool content telemetry is opt-in.

Default telemetry prefers:

- IDs,
- hashes,
- sizes,
- counts,
- timing,
- redacted structured attributes.

Support separate local debugging modes with explicit warning/retention.

## 11. Research and downloaded artifacts

External research may itself carry licensing/redistribution constraints.

AER SHOULD store only what is needed for provenance/analysis and avoid treating fetched copyrighted content as freely redistributable project output.

## 12. Export/inspection

Users should be able to inspect:

```text
what data exists
where it came from
why it is retained
sensitivity
provider transmissions
expiry
```

Enterprise operation eventually requires project export/deletion tooling.

## 13. Tests

Data-governance tests include:

- restricted data routed away from ineligible provider,
- secret redaction,
- artifact expiry and broken-proof status,
- cross-project retrieval isolation,
- telemetry content-off mode,
- remote worker data-class denial,
- GC preserves referenced/pinned artifacts.
