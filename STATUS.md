# AER Implementation Status

**Last updated:** 2026-08-15  
**Architecture baseline:** `docs/` on original `main` commit `6c81fa1d0d18e9f279fe1bc59f56d21f2cbffd55`  
**Current phase:** Phase 1 — Durable, Safe Single-Agent Runtime  
**Current step:** 03 / 18 — Durable State Kernel  
**Repository-side state:** CI VERIFIED — awaiting target Windows verification  
**Step 01 verified implementation:** `6495c77dbc05d7db635062a35bb3bc0eb0857922`  
**Step 01 verified CI:** `foundation-ci` run `31899011790`  
**Step 02 verified implementation:** `6f9c4258299a5f9880cdec78c976aaa56bfb884d`  
**Step 02 dependency-lock commit:** `85812c58b8eb0db9e19d73313e6e59d2e46cf057`  
**Step 02 verified CI:** `foundation-ci` run `31903313314`  
**Step 02 target Windows verification:** PASS — user-reported local run on `C:\Users\cenke\OneDrive\Desktop\everything`; visible workspace tests, documentation integrity, and Phase-0 gate all green with no reported format/Clippy failure  
**Phase 0:** COMPLETE  
**Step 03 verified implementation:** `c8f1f6153cc076a6e4c1b93e8c8d6da903a80fa5`  
**Step 03 dependency-lock commit:** `e140f40e791d6fd55a8f82a580008c46ce8dcb53`  
**Step 03 verified CI:** `foundation-ci` run `31904709865`  
**Next gate:** reproduce the Step-03 locked verification commands on the target Windows checkout  
**Next step:** 04 — Runtime State + Resource Safety — BLOCKED until target Windows verification passes

## Step 02 — Executable Contract System

**State:** COMPLETE

Repository CI passed on Linux and Windows and the target Windows checkout reproduced the Step-02
verification sequence. The supplied local output showed all `aer-contracts`, `aer-domain`,
`aer-doc-check`, and `aer-phase0-check` tests passing, followed by green documentation-integrity
and Phase-0 executable-contract reports. Phase 0 is closed.

## Step 03 scope

Implemented and repository-verified:

- dedicated `aer-storage` crate with no provider/model dependency;
- SQLite database schema v1 activated as an independent compatibility surface;
- read-only fail-closed durable-state preflight before mutation of existing state;
- refusal to claim unrelated SQLite databases or foreign non-empty `.aer` directories;
- SQLite WAL mode, `synchronous=FULL`, foreign-key enforcement, explicit busy timeout, and trusted-schema hardening;
- transactional baseline migration with immutable migration-history identity/checksum;
- cleanup/retry semantics for failed initial migration and fail-closed future-version handling;
- project-scoped SHA-256 content-addressed object storage with file-before-metadata ordering;
- atomic temporary-file write + fsync + rename object persistence;
- ordinary object-store rejection of `secret` data;
- project-scope enforcement on artifact reads and event references;
- append-only event journal with SQL triggers rejecting update/delete;
- monotonic in-process ULID event generation plus authoritative SQLite sequence ordering;
- internal causation resolution, explicit external causation, and cross-project causation rejection;
- event + materialized journal projection updated in one SQLite transaction;
- deterministic replay digest, projection verification, and rebuild from immutable events;
- artifact-integrity verification against referenced content hashes;
- crash-boundary tests for initial migration, object-file-before-metadata, and event-before-projection failure points;
- reopen/replay equivalence and corruption-detection tests;
- checked-in Step-03 dependency lock and read-only locked CI on Linux + Windows.

## Provider authentication/onboarding requirement recorded for Step 06

`docs/37_PROVIDER_GATEWAY_AND_RESILIENCE.md` specifies provider authentication as a first-run
and settings workflow rather than hidden adapter configuration.

Key requirement: use official OAuth 2.0 + PKCE/device authorization where the provider officially
supports third-party CLI OAuth; otherwise use the provider's supported API-key/token mechanism.
AER must not fake OAuth through cookies or undocumented consumer endpoints. Raw credentials stay
out of SQLite, events, objects, logs, telemetry and prompts; persistent credentials use an OS
secure credential-store adapter and AER durable state stores only opaque references/non-secret
profile metadata. The future CLI must also support multiple profiles, re-auth/revocation,
headless non-interactive behavior, and mocked auth lifecycle tests without live paid credentials.

## Step 03 verification ledger

| Gate | State | Evidence / action |
|---|---|---|
| Phase-0 target Windows gate | PASS | User-provided local output on 2026-08-15; all visible tests and integrity gates green. |
| Architecture authority re-read | PASS | `03`, `24`, `25`, `34`, `40`, `42`, ADR-0005 and ADR-0008 re-checked before implementation. |
| Database compatibility preflight | PASS | Future/foreign durable state fixtures fail before migration mutation. |
| SQLite WAL + FULL durability | PASS | File-backed conformance tests pass under final CI on Linux and Windows. |
| Baseline migration atomicity | PASS | Injected pre-commit failure leaves no partially claimable v1 database; clean retry succeeds. |
| Migration identity/checksum | PASS | Existing v1 state is checked against migration identity/checksum before normal write mode. |
| Object hashing/idempotence | PASS | Same bytes map to one SHA-256 identity; content is re-hashed on read. |
| Secret exclusion | PASS | `secret` metadata is rejected by the ordinary object-store API. |
| Project object scope | PASS | Cross-project reads/references fail even when physical content is deduplicated. |
| Event immutability | PASS | SQL triggers reject UPDATE and DELETE against journal history. |
| Event causation integrity | PASS | Unknown and cross-project internal causes fail; explicit external causes remain distinct. |
| Transactional event/projection ordering | PASS | Injected fault after event insert rolls back event and projection together. |
| Replay equivalence | PASS | Materialized head equals deterministic replay; induced drift is detected and rebuildable. |
| Artifact corruption detection | PASS | Referenced bytes whose content hash changes fail project-integrity verification. |
| Reopen/recovery | PASS | Close/reopen preserves schema identity, object integrity, and replay equivalence. |
| Dependency lock | PASS | CI resolved and committed `Cargo.lock` at `e140f40e...`; bootstrap write permission was then removed. |
| CI permission hardening | PASS | Final workflow has `contents: read`; no lock-writing/bootstrap step remains. |
| GitHub Linux CI | PASS | `foundation-ci` run `31904709865`: format, locked Clippy, workspace tests, storage gate, docs and Phase-0 regression all passed. |
| GitHub Windows CI | PASS | `foundation-ci` run `31904709865`: format, locked Clippy, workspace tests, storage gate, docs and Phase-0 regression all passed. |
| Target Windows verification | PENDING | Pull `main` and run the Step-03 commands in `README.md`. |

## Step 03 CI remediation history

1. The first run generated and committed the new dependency graph as `Cargo.lock`; temporary `contents: write` permission was immediately removed.
2. Canonical Rust formatting differences were applied exactly rather than bypassed.
3. Clippy then found one runtime import that existed only for a test helper; the import was removed from the production surface instead of suppressing the warning.
4. Final run `31904709865` passed every gate on both Ubuntu and Windows with the checked lockfile and read-only workflow permissions.

## Step 03 architectural ordering decision

Artifact bytes are durably written and synchronized before their database metadata can commit.
This deliberately permits a crash to leave an unreferenced orphan file, because that state is
recoverable by later GC/re-registration. The inverse state — committed authoritative metadata or
event references pointing to bytes that never became durable — is not permitted.

Events and their current journal projection are committed in the same SQLite transaction. The
event stream remains authoritative and immutable; the projection is disposable derived state that
can be verified or rebuilt deterministically.

## Step 03 exit condition

Repository-side Step-03 gates are satisfied. Do **not** start Step 04 until the target Windows
checkout reproduces the Step-03 verification commands. Any local-only crash/replay/compatibility
failure is a Step-03 defect; fix and record it here rather than weakening the gate.
