# Environment Reproducibility and Software Supply Chain

## 1. Objective

A patch cannot be called verified if AER cannot identify the environment, dependencies, and build inputs under which the evidence was produced.

This subsystem owns:

- environment discovery/fingerprinting,
- toolchain and package-manager identity,
- lockfile/dependency discipline,
- build/test environment reproduction,
- third-party dependency risk,
- SBOM/provenance hooks,
- artifact signing/verification hooks.

## 2. Environment Fingerprint

Evidence references a typed `EnvironmentFingerprint` defined by `schemas/environment-fingerprint.schema.json`.

At minimum capture relevant subsets of:

```text
os / version
architecture
kernel/runtime
shell
filesystem semantics
sandbox backend + image/digest
language toolchains
package managers
lockfile hashes
compiler/build tool versions
service/container image digests
selected environment variable names + redacted value hashes where safe
locale/timezone
network policy
hardware accelerator identity when behavior depends on it
```

Do not include raw secrets.

## 3. Reference profiles

Projects MAY define named verification profiles such as:

```text
dev
ci-linux-x86_64
ci-windows-x86_64
release
gpu-reference
```

Acceptance criteria can bind to one or more profiles.

“Works on my sandbox” is insufficient when the project declares cross-platform targets.

## 4. Dependency installation policy

Package installation is a controlled external-input operation.

Default rules:

- respect existing lockfiles;
- prefer frozen/locked installs for verification;
- do not silently upgrade unrelated dependencies;
- do not use floating `latest` for durable/release evidence;
- record registry/source and resolved version;
- package-manager network access goes through network policy;
- new direct dependencies require the assessment defined in `26`.

A model MAY propose a dependency. Deterministic tooling resolves and records it.

## 5. Dependency assessment

For new or materially upgraded dependencies record, where available:

```text
name/version
source registry/repository
integrity hash
license
direct/transitive
reason
maintenance signal
known security advisories
native/build-script capability
replacement alternatives
```

Higher-risk packages trigger stronger sandbox/verification.

## 6. Build scripts and package hooks

Dependency installation may execute arbitrary code.

Treat:

- npm lifecycle scripts,
- Python build backends,
- Cargo build scripts,
- package post-install hooks,
- downloaded binaries,
- compiler plugins,

as executable supply-chain input.

Sandbox authority and network/credential restrictions remain active during install/build.

## 7. SBOM and provenance

AER SHOULD support optional release/build metadata compatible with ecosystem standards rather than inventing a proprietary SBOM format.

Current baseline references:

- SLSA v1.2 provenance: https://slsa.dev/spec/v1.2/provenance
- SPDX 3.0: https://spdx.dev/use/specifications/
- in-toto attestation framework: https://in-toto.io/docs/specs/
- Sigstore/Cosign for signing/verification: https://docs.sigstore.dev/

Adoption is progressive; v1 does not require every local debug build to emit full attestations.

## 8. Artifact provenance

Release-grade artifacts SHOULD be traceable to:

- source commit,
- Engineering IR/release requirement refs,
- build definition,
- environment fingerprint,
- dependency lock/SBOM,
- build evidence,
- signer/identity when signing is enabled.

A model-generated “release succeeded” message is not provenance.

## 9. Reproducibility levels

Use explicit labels:

```text
identified      # inputs/environment fingerprinted
repeatable      # same environment can re-run successfully
reproducible    # independent clean environment reaches equivalent result
hermetic        # undeclared inputs/network are blocked
```

Do not claim higher levels without evidence.

## 10. Cache safety

Build/test caches key on dependency/environment fingerprints.

A cache hit from a mismatched compiler, lockfile, feature set, environment or platform MUST NOT be treated as fresh evidence.

## 11. Vulnerability freshness

SBOM is a snapshot; vulnerability knowledge changes later.

Security review MAY re-evaluate existing dependency inventories without rebuilding the artifact.

A later advisory can invalidate security acceptance while leaving the original build evidence historically true.

## 12. Tests

AER needs fixtures for:

- lockfile drift,
- malicious install hooks,
- dependency confusion,
- changed registry content/integrity hash,
- cross-platform fingerprint differences,
- stale build caches,
- reproducible clean builds,
- SBOM/provenance generation and verification where enabled.
