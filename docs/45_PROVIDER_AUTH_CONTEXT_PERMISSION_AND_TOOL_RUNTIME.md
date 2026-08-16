# Provider Authentication, Model Context, Permission and Tool Runtime

**Status:** Normative product/runtime specification  
**Scope:** production model onboarding, delegated OAuth sessions, provider transports, architecture context, tool authority, permission UX, model I/O evidence, and live smoke verification.

## 1. Objective

`everything` must be able to use real coding-model subscriptions and APIs without turning provider CLIs into the product architecture.

The system therefore separates four concerns that are often incorrectly collapsed:

```text
authentication session
        !=
provider transport
        !=
model context
        !=
tool / side-effect authority
```

A provider can authenticate the user and generate model output while AER still owns what repository state the model sees, which tools exist, what side effects are possible, and what evidence is required before acceptance.

The first production target is OpenAI Codex, Anthropic Claude Code, and Google Gemini CLI. Provider-specific behavior lives behind adapters and MUST NOT leak into Engineering IR, task state, verification, or permission semantics.

---

## 2. Delegated OAuth: vendor owns the secret

For consumer/developer subscription login, prefer the provider's documented login implementation rather than copying its OAuth token into AER.

```text
user
  │
  ├─ everything provider login codex
  │      └─ official Codex ChatGPT OAuth / device-code flow
  │
  ├─ everything provider login claude
  │      └─ official Claude Code browser auth flow
  │
  └─ everything provider login gemini
         └─ official Gemini CLI Sign in with Google flow
```

Rules:

1. The vendor process owns browser authorization, refresh, expiry handling and its credential cache/keychain entry.
2. AER MUST NOT scrape browser cookies, reverse-engineer consumer OAuth endpoints, copy refresh tokens, or parse undocumented credential databases.
3. AER stores only non-secret provider/profile observations that are needed for routing/audit.
4. An authentication-status command is advisory. The smallest real model call is the authoritative local connectivity check because a cached credential can still be invalid server-side.
5. API-key/direct-API profiles remain valid future transports, but are separate credential sources and never silently replace a requested subscription login.
6. Provider logout/revocation behavior is exposed only when the vendor has a supported operation. Unsupported operations are reported, not fabricated.

### 2.1 Current provider login semantics

| Provider | Local login entry | Headless alternative | Secret owner |
|---|---|---|---|
| Codex | ChatGPT browser OAuth | official device-code login | Codex |
| Claude Code | browser `auth login` | provider-supported non-interactive credentials where explicitly configured | Claude Code |
| Gemini CLI | interactive `Sign in with Google` browser flow | API key / Vertex credentials when explicitly selected | Gemini CLI / Google |

Gemini currently requires its interactive authentication selector for Google-account sign-in. `everything provider login gemini` launches that official UX rather than pretending a separate stable login command exists.

---

## 3. Provider transport is independent of authentication

Authentication answers **who may call the provider**. Transport answers **how AER exchanges typed model events**.

Target transport order:

1. structured provider control protocol / SDK that allows AER to mediate tools and approvals;
2. supported headless machine-readable CLI mode;
3. direct provider API when explicitly configured and policy-eligible.

Current productization vertical slice:

| Provider | Initial transport | Machine output | Smoke authority |
|---|---|---|---|
| Codex | `codex exec` | JSONL events | ephemeral, read-only sandbox, headless approval-never |
| Claude Code | `claude -p` | JSON | built-in tools removed, plan mode, no session persistence |
| Gemini CLI | headless prompt | JSON | plan/read-only mode in an empty AER temp workspace |

Longer-term preferred transports are Codex app-server, Claude Agent SDK, and Gemini ACP because those protocols expose richer structured session/tool/approval control. The headless CLI slice exists to produce a small real working product now; it MUST NOT become a permanent reason to parse decorative terminal text.

All vendor subprocesses use fixed AER-constructed argv. Model output never supplies the executable or provider-control flags.

---

## 4. Every model receives the same architecture identity

Provider-native files (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`) are compatibility/bootstrap surfaces. They are useful but not sufficient because different providers load them differently and users can alter provider-local configuration.

The authoritative model bootstrap is an AER-generated **Architecture Context Capsule**.

Initial capsule sources:

```text
AGENTS.md
a bounded STATUS.md slice/file budget
docs/00_READ_ME_FIRST.md
DEVELOPMENT_PLAN.md
docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md when present
```

The compiler:

- is provider-neutral;
- is bounded before model dispatch;
- records source path, full-file SHA-256, total bytes, included bytes and truncation;
- derives one capsule digest;
- fails when mandatory architecture sources are missing;
- never treats repository text as authority to widen capabilities.

A model call records the exact capsule digest. Task-specific Engineering IR, repository evidence, Handoff ABI and retrieved code are added later through the Context Economy Engine rather than by dumping the whole repository into every request.

This gives all models a shared identity while preserving context economy:

```text
stable architecture capsule
        +
task handoff / Engineering IR
        +
minimal repository evidence
        +
lazy tool schemas
        =
model context
```

---

## 5. AER owns tools; providers are intelligence resources

Provider-native file/shell tools MUST NOT become an alternate authority path around AER.

The internal Tool ABI in `14_TOOLS_MCP_A2A_AND_SKILLS.md` remains canonical. MCP, provider tool formats and agent protocols are adapters.

### 5.1 Premium tool-runtime principles

The runtime should be faster and more precise than a generic “give the model a shell” loop:

1. **Structured operations first.** File reads, search, patches, Git inspection and verification use typed arguments. A raw shell string is a separate explicit tool, not the universal primitive.
2. **Progressive disclosure.** Every model starts with a tiny core catalog. It searches tool metadata and receives the full input schema only for a selected tool.
3. **Range- and artifact-oriented I/O.** Large files/output are not injected wholesale. Return bounded previews, hashes, line/range handles and artifact references.
4. **Repository-intelligence integration.** Prefer symbol/reference/impact retrieval from RI2 over repeated blind grep/tree scans.
5. **Batch safe reads.** Independent reads/searches MAY run concurrently under resource bounds. Writes reuse Step-13 dependency/write-set conflict controls.
6. **One write authority.** Workspace mutation occurs only in AER-owned isolated worktrees. Process-capable `ToolBroker` construction requires an `aer_workspace::OwnedWorktree` authority token; permission mode alone cannot authorize commands in a user-owned checkout.
7. **Command evidence.** Commands are normalized argv + cwd + environment policy + timeout + output hashes + exit/resource evidence.
8. **No hidden host shell.** A model never receives an unrestricted host-process handle or host Docker socket.
9. **Idempotency before external mutation.** External writes receive dedup identity and verification before retry.
10. **Tool output is untrusted data.** A file, web page, MCP response or compiler log cannot grant permission or alter policy.

### 5.2 Initial core catalog

The stable conceptual catalog should remain small:

```text
fs.read            bounded file/range read
fs.list            bounded directory inventory
fs.search          exact/lexical repository search
fs.patch           typed worktree patch
repo.symbol        RI2 symbol lookup
repo.references    RI2 references/backlinks
repo.impact        dependency/impact query
exec.run           structured argv command
exec.shell         explicit shell semantics, higher review surface
git.inspect        status/diff/history/branch facts
verify.run         verification profile execution
tool.search        searchable tool metadata
tool.describe      selected full schema
```

This list is an ABI direction, not authority to expose unimplemented tools as working product features. Each tool is promoted only when its typed implementation and verification exist.

---

## 6. Permission mode is not capability authority

This distinction is mandatory.

```text
capability ceiling = what this run/sandbox is technically authorized to do
permission mode    = when the user is asked within that ceiling
```

A model, prompt, repository file, provider, MCP server or `/permission` mode change cannot widen the capability ceiling.

### 6.1 User-facing modes

| Mode | Automatic behavior | Other effects |
|---|---|---|
| `plan` | pure reads | non-read actions denied |
| `default` | pure reads | every other eligible effect asks |
| `auto` | reads + isolated worktree edits + local commands | network/external/credential effects ask |
| `full` | every effect already inside the current capability ceiling | no prompt-driven privilege elevation |

The default interactive mode is `default`.

`full` means **maximum autonomy inside the established sandbox/run authority**, not “disable security”. The ordinary developer ceiling intentionally excludes `privileged`; selecting `full` cannot acquire it.

Explicit session deny rules override every mode, including `full`.

### 6.2 Interactive command

```text
/permission
/permission plan
/permission default
/permission auto
/permission full
/permission allow <effect>
/permission deny <effect>
/permission reset <effect>
```

The first implementation is session-local. Durable project/organization defaults belong in the Configuration and Policy Model after schema/policy promotion; they MUST NOT be persisted ad hoc in shell history or provider config.

### 6.3 Permission request object

A non-read action that needs a decision must produce a typed request containing at least:

```text
side_effect_class
target
reason
reversible
risk class
scope / duration
least-authority alternative when one exists
```

The normal UI may be concise, but this object is the audit truth.

---

## 7. Model-call I/O receipt

A user should be able to prove which intelligence resource was actually used without exposing secrets or hidden reasoning.

A live call receipt records:

```text
provider
transport
requested/resolved model when known
architecture_context_digest
user/task input identity
final output
input/output token usage when provider reports it
duration
raw structured-event count/provider request id when available
```

Rules:

- do not log OAuth tokens, API keys, provider credential files or secret environment values;
- do not expose chain-of-thought or provider-internal hidden reasoning;
- raw provider JSON MAY be retained only as bounded/redacted diagnostic evidence under explicit inspection policy;
- normal terminal output shows the final answer plus concise usage/latency/context identity;
- `--json` emits machine-readable receipt data.

The initial real call surface is:

```text
everything provider status [codex|claude|gemini]
everything provider login <provider>
everything provider login codex --device
everything provider smoke <provider> --show-input --prompt "..."
everything provider smoke <provider> --json --prompt "..."
```

Core CI never performs these live calls.

---

## 8. Live smoke is a product acceptance layer, not a unit test

Deterministic CI tests:

- provider alias/descriptor mapping;
- machine-output parsers;
- bounded capture/timeout behavior;
- context capsule digest/bounds;
- permission lattice and explicit-deny precedence;
- CLI parsing;
- no secret material in deterministic fixtures.

Target-machine live smoke verifies what mocks cannot:

1. official vendor executable is installed;
2. delegated OAuth/session really works on that machine;
3. the provider accepts a model request;
4. the architecture capsule is transmitted;
5. machine output parses into a final answer and usage receipt.

Authentication status alone cannot satisfy this gate.

A failed live smoke is evidence about the local provider/auth/transport and MUST NOT corrupt project state.

---

## 9. Performance model

Provider support must not slow ordinary local CLI interaction.

- no provider process at normal `everything` startup;
- no auth/model discovery unless a provider capability is requested;
- no loading all provider schemas into every model call;
- no reading all architecture docs into every prompt;
- smoke/process output hard-bounded;
- provider subprocess timeout hard-bounded;
- architecture capsule compiled once per relevant source snapshot and eligible for exact-digest caching later;
- future long-running provider protocol processes MAY be pooled only after benchmark evidence shows startup savings exceed lifecycle complexity.

The first slice prefers a process-per-smoke call because it is simpler, auditable and correct. Persistent app-server/ACP/SDK sessions become the optimization path for actual agent loops.

---

## 10. Security invariants

- Vendor OAuth secrets remain outside AER normal state/context.
- Secret environment variables are not inherited by delegated smoke subprocesses by default.
- The model cannot alter permission mode or capability ceiling through text.
- Provider-native tools cannot bypass the AER Tool ABI in accepted agentic execution.
- Smoke runs are non-mutating and occur in an AER temp workspace.
- External side effects require the AER side-effect classifier regardless of provider permission semantics.
- A provider's `yolo`, bypass or equivalent mode is never interpreted as AER authority.
- Verification evidence remains independent of generator/model permission.

---

## 11. Evolution path

The provider runtime should evolve vertically rather than by adding provider-specific orchestration forks:

```text
current: vendor OAuth + bounded headless inference smoke
   ↓
structured persistent transports (Codex app-server / Claude SDK / Gemini ACP)
   ↓
AER Tool ABI exposed through provider-native tool adapters
   ↓
permission callback bridge to one AER PermissionController
   ↓
streaming model events + cancellation + provider usage/cost reconciliation
   ↓
router selects any eligible provider transport using existing Step-11 policy
```

At every stage the router, Engineering IR, context/provenance, tool authority and verification system remain provider-neutral.

---

## 12. Primary-source basis

Implementation decisions should be revalidated against current official provider documentation during adapter changes. The initial design was checked against:

- OpenAI Codex official app-server/login and `codex exec` sources/documentation;
- Anthropic Claude Code official authentication, CLI/headless, permission-mode and tool-availability documentation;
- Google Gemini CLI official authentication, CLI configuration, policy/approval and ACP documentation.

Provider CLIs change quickly. Capability/flag drift is a typed adapter compatibility failure, never silently ignored.
