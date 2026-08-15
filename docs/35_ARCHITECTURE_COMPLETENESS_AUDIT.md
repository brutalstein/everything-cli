# Architecture Completeness Audit and Gap-Closure Map

**Status:** Normative completeness review  
**Audit date:** 2026-08-15  
**Scope:** All architecture docs `00`–`34`, accepted ADRs, executable schemas, examples, and the premium CLI/TUI specification.

## 1. Purpose

This document exists to prevent a common failure mode in large agent projects: a design can be sophisticated in the areas it discusses while still having unowned gaps between subsystems.

The audit therefore asks a stricter question than “is each component well designed?”:

> For every externally observable behavior, durable state transition, resource, trust boundary, and artifact, is there an explicit owner, lifecycle, failure policy, compatibility policy, and verification path?

AER MUST periodically repeat this audit as the architecture evolves.

## 2. Audit dimensions

The architecture was reviewed across:

- human intent and requirements,
- Engineering IR and executable contracts,
- repository/context intelligence,
- model/provider execution,
- orchestration and resource scheduling,
- tool/sandbox authority,
- external research and knowledge,
- environment/dependency reproducibility,
- VCS/workspace ownership,
- evidence/verification,
- architecture health,
- durability/replay,
- data governance,
- release/update/migration lifecycle,
- domain-specific verification,
- observability/evaluation,
- CLI/TUI and headless compatibility,
- cross-platform behavior,
- self-evolution boundaries.

## 3. Gaps found and closure

| Gap | Why it matters | Closure |
|---|---|---|
| `research` was a task type but had no authoritative acquisition pipeline | Retrieved web/docs can be stale, contradictory, or adversarial | `36_RESEARCH_AND_EXTERNAL_KNOWLEDGE.md` + `research-artifact.schema.json` |
| Provider abstraction lacked operational failure semantics | Rate limits, partial streams, retries, provider drift and failover can corrupt cost/state or duplicate work | `37_PROVIDER_GATEWAY_AND_RESILIENCE.md` |
| Environment fingerprint was referenced but not defined | Evidence cannot be reproduced or safely cached without environment identity | `38_ENVIRONMENT_REPRODUCIBILITY_AND_SUPPLY_CHAIN.md` + `environment-fingerprint.schema.json` |
| Dependency installation/supply-chain policy was too shallow | Autonomous package installation is a material security and reproducibility boundary | `38` |
| Parallelism had limits but no complete admission/backpressure model | Unbounded queues/workers/provider requests can collapse a local runtime | `39_SCHEDULER_RESOURCE_GOVERNOR_AND_BACKPRESSURE.md` + ADR-0007 |
| Durable/wire schemas had version numbers but no full compatibility lifecycle | Old state, clients, events and IR must survive upgrades predictably | `40_VERSIONING_MIGRATIONS_AND_RELEASE_SAFETY.md`, `44_EXECUTABLE_CONTRACTS_AND_SCHEMA_DISCIPLINE.md`, ADR-0008 |
| Product self-update/release integrity was unspecified | A tool-executing daemon must not trust unsigned or rollbackable updates | `40` |
| Dirty working trees and user-owned git state were underspecified | Autonomous work must never overwrite or silently stash user changes | `41_WORKSPACE_VCS_AND_CHANGE_LIFECYCLE.md` |
| Retention/tenancy/privacy labels existed only as fragments | Long-lived artifacts, prompts, traces and learning data need lifecycle rules | `42_DATA_GOVERNANCE_RETENTION_AND_TENANCY.md` |
| Verification architecture was generic but project domains differ | Web, backend, systems, data/ML and infrastructure need different executable evidence | `43_DOMAIN_CAPABILITY_AND_VERIFICATION_PROFILES.md` |
| Several central typed artifacts had no executable schema | “Typed ABI” is not enforceable if critical objects remain prose-only | `44` plus new schemas for Context Pack, WorkResult, Proof Manifest, Budget, Run, Policy Artifact |
| CLI UX configuration examples conflicted with the strict config schema | `ui.*` keys would be rejected despite being normative UX | config schema corrected |

## 4. Completeness invariant

For every new subsystem or public feature, a coding agent MUST answer:

1. **Semantic owner:** which requirement/IR object defines it?
2. **State owner:** where is its authoritative state?
3. **Authority:** who may mutate it?
4. **Lifecycle:** create → use → invalidate/retire/delete.
5. **Identity:** what stable/versioned ID names it?
6. **Failure:** what happens on timeout, crash, partial result, cancellation?
7. **Resource model:** what is bounded and how is backpressure applied?
8. **Security/data class:** what can it read/write/transmit?
9. **Compatibility:** what happens across AER upgrades?
10. **Evidence:** how is successful behavior verified?
11. **Observability:** how can the decision be inspected after the run?
12. **Recovery:** how is an interrupted operation resumed or safely repeated?

If two or more answers are missing for a high-impact feature, implementation SHOULD stop for a design spike/ADR rather than invent hidden behavior.

## 5. Intentionally deferred, not missing

The following remain deliberate non-goals until measured need exists:

- multi-host distributed consensus/control plane,
- Kubernetes-first deployment,
- built-in production CD with unrestricted deployment authority,
- foundation-model training,
- general plugin marketplace,
- cloud multi-tenant SaaS control plane,
- IDE/desktop replacement,
- mandatory graph database,
- fixed multi-agent role organization.

Do not “close” these gaps by adding infrastructure prematurely.

## 6. Documentation completeness tests

CI SHOULD eventually verify:

- all docs referenced from `00_READ_ME_FIRST.md` exist;
- all accepted ADR references resolve;
- all JSON Schemas parse;
- shipped examples validate against their declared schemas;
- internal Markdown links/paths resolve;
- `MANIFEST.sha256` is current;
- no normative config example uses a key rejected by the config schema;
- every core contract listed in `44` has either an executable schema or an explicitly documented Rust/protobuf authority;
- schema compatibility fixtures cover supported upgrade paths.

## 7. Re-audit triggers

Repeat this audit when:

- a new public client or remote worker is introduced;
- a new durable store is added;
- production deployment authority is added;
- a new external protocol becomes core rather than adapter-only;
- a new cross-project learning mode is introduced;
- a major Engineering IR version ships;
- a new release channel/updater mechanism ships;
- architecture-health or verification semantics materially change.
