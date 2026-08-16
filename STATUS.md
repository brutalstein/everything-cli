# everything Implementation Status

**Last updated:** 2026-08-16  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Public product / executable:** `everything`  
**Internal architecture terminology:** AER remains valid where the architecture uses it  
**Current phase:** Phase 4 — Verification and Proof System  
**Current step:** 10 / 18 — Verification + Proof System  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-10 code HEAD:** `c48a9afa95e63467198a0ea251c100232f90b79b`  
**Verified Step-10 CI:** `foundation-ci` run `31939146224` — Ubuntu PASS including permanent Verification + Proof gate; canonical isolated Windows verifier PASS  
**Next step:** 11 — Provider Resilience + Cost Router — BLOCKED until Step-10 target Windows verification passes

## Agent engineering policy

`AGENTS.md` is the canonical implementation temperament for coding agents. YAGNI, semantic DRY, dependency restraint, bounded resource use, fail-closed correctness, evidence-before-completion, and measured performance apply to all remaining implementation work. `CLAUDE.md` delegates to it rather than duplicating policy.

## User-directed product-surface freeze

The CLI/TUI remains intentionally frozen while the core architecture is completed. Until the user explicitly lifts this rule:

- do not add or redesign CLI/TUI features;
- do not expose new core capabilities through `crates/aer-cli`;
- do not use presentation work as a Step exit criterion;
- preserve the existing zero-redraw CLI only as a regression surface;
- develop and verify domain/core/storage/repository/context/runtime architecture first.

`crates/aer-cli/**` was not modified by Step 10.

## Completed milestones

- **Step 01 — Foundation Bootstrap:** COMPLETE — CI `31899011790`.
- **Step 02 — Executable Contract System:** COMPLETE — CI `31903313314`; target Windows PASS.
- **Phase 0:** COMPLETE.
- **Step 03 — Durable State Kernel:** COMPLETE — CI `31905250522`; target Windows PASS.
- **Step 04 — Runtime State + Resource Safety:** COMPLETE — CI `31906368065`; target Windows PASS.
- **Step 05 — Workspace + Execution Boundary:** COMPLETE — CI `31909059844`; target Windows PASS.
- **Step 06 — Single-Agent Runtime 0.1:** COMPLETE — CI `31911224304`; target Windows PASS.
- **Step 07 — Intent + Research + Engineering IR:** COMPLETE — semantic baseline `d5668b5d87a3b8a3f598b9cd016cc11cc5504837`; target Windows reproduction confirmed.
- **Step 08 — Repository Intelligence:** COMPLETE — code HEAD `12b97c6e9c715a19354af6ba5b661eb83ed9f353`; CI `31918025079`; target Windows canonical verification reproduced by the user on 2026-08-16.
- **Step 09 — Context Economy Engine:** COMPLETE — repository CI `31920562037`; target Windows canonical verification reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.

The Step-09 target-machine reproduction also reconfirmed the checked-in documentation/contract inventory and the ContextBench/regression gates before Step 10 was started.

## Step 10 — Verification + Proof System

**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING

### Ownership and scope

The first architecture-complete verification vertical slice lives in `aer-core::verification`.

Step 10 deliberately does **not** create an `aer-verify` crate merely because the target repository map names one. The current slice already depends on core orchestration, contracts, environment identity, execution, domain state transitions, and durable storage. A separate crate should be introduced only when independent ownership, dependency pressure, or testing boundaries make that split materially clearer.

The implementation is intentionally generic. Domain-specific checks are supplied through verification profiles; domain knowledge does not fork the task state machine or weaken organization-level gates.

### Independent verifier authority

`VerifierDefinition` describes a verifier by stable ID/version, verification layer, evidence type, executable/arguments, protected verifier assets, timeout/capture bounds, and isolation requirement.

`VerifierSnapshot` binds the definition digest to a deterministic recursive digest of protected verifier/test assets. Candidate verification re-hashes those assets before execution. A changed verifier definition, changed protected test, changed verifier asset, symlinked verifier asset, or path escape fails closed.

This is the Step-10 defense against a generator obtaining a false PASS by weakening the oracle it is being judged by.

### Verification composition and Domain Profiles

`VerificationPlan` starts from mandatory verifier/evidence requirements and composes every applicable `DomainProfile` by set union.

A lower/domain profile can add gates but cannot remove a mandatory verifier or evidence type. The bound plan also pins the exact verifier snapshots used for the run and derives a deterministic composition snapshot.

### Environment-bound evidence

Verifier execution reuses the existing `aer-exec` and `aer-environment` boundaries rather than introducing a parallel process runtime.

Every produced Evidence Record is bound to:

- exact repository snapshot;
- `EnvironmentFingerprint` digest;
- verifier ID/version;
- immutable verifier snapshot;
- command argv/cwd;
- input artifact hashes;
- stdout/stderr hashes and byte counts;
- exit/timing result;
- command-evidence digest;
- declared security profile.

Strong isolation is not simulated. When a verifier requires stronger isolation than the current direct executor can provide, execution fails closed before the verifier process is admitted.

### Evidence cache boundary

`EvidenceCacheKey` treats repository snapshot, environment fingerprint, verifier snapshot, and input artifact hashes as hard reuse boundaries.

A change in any of them makes prior evidence stale. Step 10 does not claim probabilistic or semantic cache equivalence.

### Proof-carrying acceptance

`build_proof_manifest` requires exact coverage of the task's requirement set. Each requirement must map to:

1. at least one current implementation location;
2. at least one passing Evidence Record that attests that requirement;
3. evidence from the same repository snapshot;
4. evidence carrying environment and verifier-integrity identity;
5. every verifier/evidence type required by the bound Verification Plan.

The generated Proof Manifest is validated through the executable schema registry and then through the existing cross-contract semantic validator. Generator-controlled verifier evidence cannot support an accepted proof.

### Durable acceptance chain

`persist_accepted_verification` validates the proof and the domain transition before persisting acceptance.

The authoritative sequence is:

1. store Evidence Records as content-addressed internal artifacts;
2. append `evidence.created` events;
3. store the passing Proof Manifest as a pinned artifact;
4. append `verification.verdict` referencing that proof;
5. append `task.accepted` causally linked to the verification verdict.

The existing `TaskState::Verifying -> TaskState::Accepted` guard remains authoritative and requires accepted proof. The verification slice does not bypass the state machine with a generic status write.

### Step-10 adversarial and invariant tests

The focused Step-10 test surface verifies that:

- deliberate protected-test/verifier tampering is detected;
- Domain Profiles can only strengthen mandatory verification;
- repository/environment/verifier/input changes invalidate evidence reuse;
- unsupported strong-isolation requirements fail closed;
- command evidence is bound to repo/environment/verifier identity;
- Proof Manifest construction requires an exact requirement -> implementation -> passing-evidence chain;
- stale-repository evidence cannot support a current proof;
- accepted verification persists evidence -> verdict/proof -> task acceptance in causal order and preserves journal integrity.

### Permanent verification gates

Step 10 adds a permanent Linux CI gate:

```text
cargo +1.97.1 test --locked -p aer-core --all-targets verification
```

The canonical Windows verifier now includes the corresponding target-specific Step-10 gate before the remaining storage/document/Phase-0/product checks.

The final repository-side verification run `31939146224` passed:

- workspace formatting;
- workspace-wide `-D warnings` Clippy;
- full workspace regression suite;
- Intent + Research + Engineering IR gate;
- Repository Intelligence gate;
- Context Economy gate;
- Verification + Proof integrity gate;
- Single-Agent Runtime gate;
- Workspace + execution boundary gate;
- CLI regression/zero-redraw guard;
- Durable State Kernel gate;
- documentation integrity;
- Phase-0 executable contract gate;
- canonical isolated Windows verification.

Temporary branch-only format/compile repair workflows used during implementation were removed after their exact repairs. No write-capable repair workflow is part of the verified Step-10 tree.

## Step 10 acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Independent verifier definition + protected asset identity | PASS | `VerifierDefinition` + `VerifierSnapshot`. |
| Deliberate verifier/test tampering detection | PASS | `immutable_verifier_detects_deliberate_test_tampering`. |
| Safe relative verifier asset boundary | PASS | path validation + symlink/unsupported-asset rejection. |
| Mandatory verification cannot be weakened by Domain Profiles | PASS | monotone union composition + focused test. |
| Bound verifier composition snapshot | PASS | required snapshot resolution + deterministic composition digest. |
| Evidence bound to exact repository snapshot | PASS | `CommandExecutionEvidence` + Evidence Record construction. |
| Evidence bound to Environment Fingerprint | PASS | environment digest required and persisted. |
| Evidence bound to verifier identity/snapshot | PASS | verifier ID/version + integrity snapshot checks. |
| Evidence input/output artifact identity | PASS | SHA-256 input validation + captured output hashes. |
| Exact evidence cache invalidation boundary | PASS | repo/environment/verifier/input cache-key test. |
| Strong-isolation capability mismatch fails closed | PASS | direct executor refusal test. |
| Exact requirement -> implementation -> evidence coverage | PASS | proof builder coverage rules + focused proof test. |
| Stale repository evidence rejected | PASS | stale-evidence adversarial test. |
| Generator-controlled verifier evidence rejected | PASS | proof integrity guard + existing semantic validator. |
| Current Evidence Record schema validation | PASS | embedded executable contract registry. |
| Current Proof Manifest schema validation | PASS | embedded executable contract registry. |
| Cross-contract semantic proof validation | PASS | `validate_semantic_bundle`. |
| Accepted task requires passing proof | PASS | existing domain state-machine guard reused. |
| Durable evidence -> verdict/proof -> acceptance chain | PASS | persistence integration test + journal integrity verification. |
| No new third-party Step-10 dependency | PASS | implementation reuses existing workspace crates/dependencies. |
| No premature `aer-verify` crate split | PASS | YAGNI ownership decision documented above. |
| CLI/TUI freeze preserved | PASS | no `crates/aer-cli/**` changes. |
| Workspace-wide format | PASS | CI `31939146224`. |
| Workspace-wide `-D warnings` Clippy | PASS | CI `31939146224`. |
| Full workspace regression suite | PASS | CI `31939146224`. |
| Permanent Linux Verification + Proof CI gate | PASS | CI `31939146224`. |
| Canonical isolated Windows CI verifier including Step 10 | PASS | CI `31939146224`. |
| Temporary write workflow/repair scaffolding removed | PASS | verified Step-10 code HEAD `c48a9afa95e63467198a0ea251c100232f90b79b`. |
| Target Windows canonical verifier | PENDING | user reproduction required on updated `main`. |

## Step 10 exit condition

Repository-side Step 10 is verified. Do **not** start Step 11 until the target Windows checkout reproduces the canonical verifier successfully.

No interactive CLI testing is required. After Step 10 is merged to `main`, run only:

```powershell
cd C:\Users\cenke\OneDrive\Desktop\everything
git pull origin main
.\scripts\verify-windows.ps1
```

Expected final line:

```text
everything Windows verification: PASS
```

After that PASS, mark Step 10 COMPLETE and proceed to **Step 11 — Provider Resilience + Cost Router**, keeping the CLI/TUI frozen.
