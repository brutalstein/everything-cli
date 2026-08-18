# Execution Sandbox and Tool Runtime

## Goal

Give agents enough autonomy to build and test real software without giving model output unrestricted authority over the host, credentials, network, verifier, or unrelated repositories.

## Security principle

The sandbox is the primary autonomy boundary. Repeated permission prompts are not a sufficient security architecture and create approval fatigue.

## Trust levels

AER defines policy-driven execution profiles:

### `read-only`

- repository read access,
- no writes,
- network policy controlled,
- suitable for scouting/review.

### `workspace-write`

- read/write only inside isolated worktree and task temp directories,
- no host credential access,
- network deny by default,
- standard implementation default.

### `networked-build`

- workspace-write plus explicit package/registry/domain egress,
- credential operations via broker/proxy where possible.

### `privileged-special`

- rare explicit workflow requiring elevated capabilities,
- strong user/org policy,
- full audit,
- never selected silently by a model.

## Sandbox backend interface

Core exposes:

```text
CreateSandbox(policy, workspace, resources) -> SandboxHandle
Exec(handle, command) -> ProcessResult/stream
ExposePort(...)
Snapshot(...)
Destroy(...)
```

Backends are replaceable.

Possible implementations:

- Linux OS sandbox / rootless container;
- Docker Desktop / microVM isolation on Windows/macOS;
- Firecracker or equivalent managed microVM for remote workers;
- provider-native sandbox APIs.

Do not make one backend's semantics leak into the task model.

## Required isolation dimensions

1. **Filesystem** — worktree-only writable scope.
2. **Network** — deny by default; domain/endpoint policy.
3. **Credentials** — secrets stay outside model-visible filesystem when feasible.
4. **Process/resources** — CPU, memory, PIDs, disk, wall-clock limits.
5. **Host control sockets** — no host Docker socket or equivalent high-authority interface.
6. **Verifier assets** — held-out verification material mounted separately and immutable to generators.

## Git hardening

Agent-controlled repository data can affect host behavior through git configuration/hooks.

Protect or mediate:

- `.git/config`,
- hooks,
- attributes/filters where dangerous,
- credential helpers,
- remote URLs,
- submodule behavior,
- executable checkout side effects.

Remote push is a separate capability from local commit.

## Network policy

Network is classified by purpose:

```text
none
package-registry-only
research-allowlist
project-services
unrestricted (high risk; normally forbidden)
```

Requests to expand network access are policy events, not model assumptions.

## Credential broker

Where integrations require credentials, prefer an out-of-sandbox broker:

```text
agent -> scoped request -> broker -> external service
```

The model/sandbox should receive the least-authority capability necessary, preferably short-lived and destination-bound.

## Command execution

Every command execution records:

- normalized argv/shell form,
- cwd,
- selected environment metadata,
- sandbox ID/policy,
- start/end time,
- exit code/signal,
- stdout/stderr artifact hashes,
- resource usage where available.

Sensitive environment values are redacted before telemetry.

## Side-effect classification

Tools declare side effects:

```text
pure_read
workspace_write
process_execution
network_read
network_write
external_mutation
credential_use
privileged
```

Policy authorizes by classification, not by natural-language tool description alone.

## Windows and Linux

Cross-platform support is mandatory at the contract level.

Implementation MAY reach strong isolation through different backends:

- Linux: rootless/container/namespace-based sandboxing;
- Windows: WSL2/Hyper-V/Docker microVM or native constrained backend;
- macOS: VM/container or OS-supported constrained backend.

If strong isolation is unavailable, AER MUST report the degraded security profile rather than pretending equivalence.

## Current implementation truth

The typed vocabulary of this document exists in `crates/aer-exec`: `TrustLevel`, `NetworkClass`, `IsolationDimension` and `IsolationReport`. Two of those types are deliberately partial. Only the `read-only` and `workspace-write` trust levels and only the `none` and `unrestricted` network classes are defined, because no policy path selects the others yet, and a level nothing can select would be an authority claim with no implementation behind it.

Authority and enforcement are separate types on purpose. A trust level states what a run is permitted to do; an `IsolationReport` states what the substrate actually controls. Nothing derives the second from the first.

### What the available substrate enforces

The only execution substrate today is a direct host child process. Its report enforces **none** of the six required dimensions. Confining the working directory and clearing inherited environment variables is real hygiene and is implemented, but a child process can still write anywhere the user can, open any socket, and read any credential file, so claiming a dimension would be false.

### Consequence: two execution paths, one boundary

| Path | Argv author | Policy | Behavior today |
|---|---|---|---|
| AER-authored tooling — Git, provider CLI, verification commands | AER | `ExecutionPolicy::trusted_workspace` | runs, with bounded timeout, capture limits, cwd containment, environment allowlist and argv/output-hash evidence |
| Model-directed execution — the `exec.run` tool | the model | `ExecutionPolicy::sandboxed` | **fails closed**, naming the trust level, the network class and every unenforced dimension |

`exec.run` remains in the tool catalog and carries the reason it is unavailable, so a caller discovers the refusal before making the call rather than by being denied.

This satisfies the requirement above that an unavailable strong isolation is reported rather than treated as equivalent. It does not substitute for a sandbox: adding a strong backend means adding a substrate that can prove dimensions, at which point the same policy stops failing closed without any change to the tool runtime.

## Cleanup

Sandbox destruction must clean:

- subprocess trees,
- ports,
- temp secrets,
- task services,
- worktree mounts,
- transient containers/VMs.

Evidence needed for audit is copied to the content-addressed artifact store before teardown.
