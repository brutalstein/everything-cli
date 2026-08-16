from pathlib import Path
import hashlib


def append_once(path: Path, heading: str, block: str) -> None:
    text = path.read_text(encoding="utf-8")
    if heading not in text:
        if not text.endswith("\n"):
            text += "\n"
        text += "\n" + block.strip() + "\n"
        path.write_text(text, encoding="utf-8")


status = Path("STATUS.md")
text = status.read_text(encoding="utf-8")
old_header = '''**Current phase:** Phase 7 — Bounded Parallel Execution  
**Current step:** 13 / 18 — Bounded Parallel Execution  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-13 production code HEAD:** `fb6f10bb72c3dc2b84a1625a746611fcfd658381`  
**Verified Step-13 CI:** `foundation-ci` run `31961188080` — Ubuntu PASS including permanent ResourceBench; canonical isolated Windows verifier PASS  
**Next step:** 14 — Architecture Health Controller — BLOCKED until Step-13 target Windows verification passes
'''
new_header = '''**Current phase:** Inter-step Provider Runtime Productization Gate  
**Current step:** between 13 / 18 and 14 / 18  
**Repository-side state:** IMPLEMENTED — authoritative Linux/Windows CI pending  
**Provider branch:** `agent/provider-runtime-productization`  
**Provider gate:** delegated Codex/Claude/Gemini auth transports + Architecture Context Capsule + permission controller + AER ToolBroker + real-model smoke surface  
**Next step:** 14 — Architecture Health Controller — BLOCKED until provider productization target-machine live smoke closes
'''
if old_header in text:
    text = text.replace(old_header, new_header, 1)
elif new_header not in text:
    raise SystemExit("STATUS header is not in an expected state")

freeze_anchor = '`crates/aer-cli/**` was not modified by Steps 10–13.\n'
freeze_note = '\nThe user explicitly lifted the product-surface freeze for the inter-step provider/auth/tool/permission productization gate. This is a scoped exception: provider onboarding, model smoke I/O and `/permission` may evolve now; unrelated TUI redesign remains deferred.\n'
if freeze_note.strip() not in text:
    if freeze_anchor not in text:
        raise SystemExit("CLI freeze anchor missing")
    text = text.replace(freeze_anchor, freeze_anchor + freeze_note, 1)

step12 = '- **Step 12 — Repository Intelligence 2.0 + Long-Horizon Engineering State + Recovery:** COMPLETE — repository CI `31957740270`; post-merge main CI `31958367494`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.\n'
step13 = '- **Step 13 — Bounded Parallel Execution:** COMPLETE — PR CI `31961188080`; post-merge main CI `31962148919`; target Windows canonical verifier reproduced by the user on 2026-08-16 with final `everything Windows verification: PASS`.\n'
if step13 not in text:
    if step12 not in text:
        raise SystemExit("Step 12 milestone anchor missing")
    text = text.replace(step12, step12 + step13, 1)

text = text.replace(
    '**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING',
    '**State:** COMPLETE',
    1,
)
text = text.replace(
    '| Target Windows canonical verifier | PENDING | user reproduction required after Step 13 is merged to `main`. |',
    '| Target Windows canonical verifier | PASS | user reproduction on 2026-08-16; final line `everything Windows verification: PASS`. |',
    1,
)
old_exit = '''## Step 13 exit condition

Do **not** mark Step 13 complete or start Step 14 until the final production tree has passed authoritative Linux + canonical Windows CI, has been merged to `main`, and the target Windows checkout has reproduced `scripts/verify-windows.ps1` successfully.
'''
new_exit = '''## Step 13 exit condition

Step 13 is closed. The final production tree passed authoritative Linux + canonical Windows CI, was merged to `main`, post-merge CI passed, and the target Windows checkout reproduced `scripts/verify-windows.ps1` successfully on 2026-08-16.
'''
if old_exit in text:
    text = text.replace(old_exit, new_exit, 1)
elif new_exit not in text:
    raise SystemExit("Step 13 exit condition is not in an expected state")

provider_section = r'''
## Inter-step Provider Runtime Productization Gate

**State:** IMPLEMENTED — AUTHORITATIVE REPOSITORY CI + TARGET LIVE SMOKE PENDING

This is a non-numbered gate between Step 13 and Step 14. It productizes capabilities deliberately left reference-only while the safety, proof and scheduling backbone was being established. It does not create a nineteenth roadmap step.

### Authentication and provider transport

- Codex, Claude Code and Gemini CLI are represented by typed delegated provider adapters.
- Subscription login remains vendor-owned: AER launches documented provider authentication UX but does not scrape browser cookies, copy refresh tokens, or parse undocumented credential databases.
- Codex supports browser ChatGPT login and official device-code login through the delegated adapter.
- Claude delegates to `claude auth login/status/logout`.
- Gemini delegates Google sign-in to Gemini CLI's interactive authentication UX; because Gemini does not expose a stable standalone non-interactive Google-login status command, the smallest read-only model call is the authoritative connectivity check.
- Live smoke calls use bounded, machine-readable headless transports and run in an AER temporary workspace with provider-native mutation disabled/plan-only.
- Secret API-key environment variables are not inherited by delegated subscription smoke subprocesses by default.

### Shared model identity

`ArchitectureContextCapsule` gives every provider the same bounded, provider-neutral architecture identity before task-specific context. The capsule is source-hashed and currently binds `AGENTS.md`, `STATUS.md`, `docs/00_READ_ME_FIRST.md`, `DEVELOPMENT_PLAN.md`, and the provider/tool runtime specification when present.

Provider-native compatibility files (`CLAUDE.md`, `GEMINI.md`, and provider-native support for `AGENTS.md`) remain convenience bootstraps; they are not the authority boundary.

### Permission controller

AER now has a typed session permission controller with four user-facing modes:

- `plan`: reads only; non-read actions denied;
- `default`: reads automatic; every other eligible side effect asks;
- `auto`: reads, isolated-worktree writes and local process execution automatic; higher-impact effects ask;
- `full`: all actions already inside the runtime capability ceiling automatic.

`full` cannot grant privileged host authority. Explicit session deny overrides every mode. `/permission` exposes mode and session override control without allowing model text/provider output to widen the ceiling.

### AER ToolBroker vertical slice

The first native tool hot path is real, typed and bounded:

- `fs.read` — canonical workspace-contained bounded line/range reads with content hash;
- `fs.list` — deterministic bounded directory inventory;
- `exec.run` — structured argv/cwd execution through the existing execution policy with timeout, bounded previews and stdout/stderr hashes;
- `tool.search` — small metadata search without injecting all schemas;
- `tool.describe` — full schema only for the selected tool.

Provider-native tools are deliberately disabled during the initial real-model smoke. The next protocol-level agent loop must bridge structured provider tool proposals to this AER ToolBroker; it must not parse decorative terminal output or treat provider YOLO/bypass modes as AER authority.

### Product surface

The provider surface remains lazy and does not spawn/discover providers during ordinary CLI startup:

```text
everything providers
everything provider status [codex|claude|gemini]
everything provider login <provider>
everything provider login codex --device
everything provider logout <provider>
everything provider smoke <provider> --show-input --prompt "..."
everything provider smoke <provider> --json --prompt "..."
```

The interactive shell exposes `/providers`, `/provider ...`, and `/permission ...` equivalents.

### Acceptance ledger

| Gate | State | Evidence |
|---|---|---|
| Step 13 target Windows reproduction | PASS | user-provided canonical verifier log, 2026-08-16. |
| Delegated provider descriptors and aliases | PASS | `aer-provider::delegated` tests. |
| Vendor-owned auth boundary; no OAuth token scraping/storage | PASS | delegated adapter + normative docs. |
| Bounded Architecture Context Capsule | PASS | deterministic source/digest/budget test. |
| Default read-auto / non-read-ASK policy | PASS | permission-controller unit test. |
| Full autonomy cannot create Privileged authority | PASS | capability-ceiling unit test. |
| Explicit session deny overrides full | PASS | permission-controller unit test. |
| Bounded exact-limit provider capture | PASS | exact-limit/overflow adversarial test. |
| Codex JSONL final-output/usage parser | PASS | delegated parser test. |
| Claude/Gemini JSON final-output parser | PASS | delegated parser test. |
| Progressive Tool ABI disclosure | PASS | `tool.search`/`tool.describe` tests. |
| Structured `exec.run` command evidence | PASS | Auto-mode real `git --version` ToolBroker test. |
| Default mode command approval request | PASS | ToolBroker permission test. |
| Plan mode command denial | PASS | ToolBroker permission test. |
| Lazy provider CLI routing | PASS | provider CLI unit test. |
| Full workspace `-D warnings` Clippy | PASS | implementation CI before final permanent gates. |
| Full workspace unit/regression tests | PASS except docs inventory before manifest refresh | runtime tests passed; manifest subsequently regenerated. |
| Docs manifest covers provider runtime specification | PASS | regenerated `docs/MANIFEST.sha256`. |
| Permanent Linux provider/permission/tool gate | PENDING | final authoritative PR CI. |
| Canonical Windows provider/permission/tool gate | PENDING | final authoritative PR CI. |
| Temporary provider repair/finalization workflows removed | PENDING | required before final PR acceptance. |
| Target Windows canonical verifier after merge | PENDING | user reproduction required. |
| Target-machine delegated OAuth + real model smoke | PENDING | at least one provider required to close this gate; all available requested providers should be exercised. |

## Provider productization exit condition

Do **not** start Step 14 until the clean production tree passes authoritative Linux + canonical Windows CI, is merged to `main`, the target Windows checkout passes the canonical verifier, and at least one delegated provider completes a real authenticated model smoke showing the final input/output receipt and architecture-context identity. Provider-specific local unavailability should be recorded explicitly rather than fabricated.
'''
if '## Inter-step Provider Runtime Productization Gate' not in text:
    text = text.rstrip() + '\n\n' + provider_section.strip() + '\n'
status.write_text(text, encoding="utf-8")

append_once(Path("docs/37_PROVIDER_GATEWAY_AND_RESILIENCE.md"), "## Delegated subscription authentication and production transport", r'''
## Delegated subscription authentication and production transport

Step 11's routing/fault/cost semantics remain authoritative. The real local subscription transport introduced after Step 13 is specified in `45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md`.

Authentication capability declarations in this document describe what a provider/profile can support; they do not imply that AER should copy a consumer OAuth token into its own store. For Codex, Claude Code and Gemini CLI subscription profiles, prefer vendor-owned delegated sessions. Routing sees non-secret auth/health/capability observations while the vendor owns browser authorization and refresh material.

A provider transport can be production-eligible only when it preserves AER context, permission, tool and evidence authority. Provider-native permission bypasses are never equivalent to AER `full` mode.
''')

append_once(Path("docs/14_TOOLS_MCP_A2A_AND_SKILLS.md"), "## Implemented native ToolBroker baseline", r'''
## Implemented native ToolBroker baseline

The first native Tool ABI hot path is implemented in `aer-core::tools` and follows the authority rules in `45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md`.

The implemented baseline is deliberately small: `fs.read`, `fs.list`, structured `exec.run`, `tool.search`, and `tool.describe`. Reads/listings/command output are bounded; `exec.run` produces argv/cwd/exit/timeout/output-hash evidence; tool schemas use progressive disclosure. MCP/A2A/provider-native tools remain adapters around this internal authority, not competing execution paths.

The first delegated provider smoke keeps provider-native tools disabled. A later structured protocol bridge may submit tool proposals to the ToolBroker, but provider output cannot execute host actions directly.
''')

append_once(Path("docs/23_CLI_AND_USER_EXPERIENCE.md"), "## Provider onboarding and session permission controls", r'''
## Provider onboarding and session permission controls

The user explicitly opened a scoped product-surface exception for real provider onboarding and permission control. These commands are lazy: normal CLI startup does not probe providers or spawn model processes.

```text
everything providers
everything provider status [provider]
everything provider login <codex|claude|gemini>
everything provider login codex --device
everything provider smoke <provider> --show-input --prompt "..."
everything provider smoke <provider> --json --prompt "..."
```

The interactive shell adds `/providers`, `/provider ...`, and `/permission ...`.

`/permission default` is the interactive default: reads are automatic and other eligible actions ask. `plan`, `auto`, and `full` adjust prompt/autonomy behavior but never widen the runtime capability ceiling. The UI must present `full` as maximum autonomy inside existing authority, not as a security bypass.
''')

append_once(Path("docs/29_CONFIGURATION_AND_POLICY_MODEL.md"), "## Session permission mode versus durable policy", r'''
## Session permission mode versus durable policy

The first `/permission` implementation is intentionally session-local. It must not persist user choices in provider-specific config, shell history, or an ad-hoc dotfile.

When project/organization defaults are promoted later, they belong in this configuration/policy model with normal precedence, schema validation, provenance and migration rules. A durable default may narrow prompting behavior only within the capability ceiling established by organization/project/run/sandbox policy; it cannot grant authority that a higher-precedence layer denies.
''')

windows = Path("scripts/verify-windows.ps1")
text = windows.read_text(encoding="utf-8")
anchor = '''    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-provider", "--test", "provider_router_bench", "--target", $Target
    )
'''
block = '''    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-provider", "--test", "provider_router_bench", "--target", $Target
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-provider", "--target", $Target, "delegated"
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-core", "--target", $Target, "permissions"
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-core", "--target", $Target, "model_context"
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-core", "--target", $Target, "tools"
    )
    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "everything", "--target", $Target, "provider_cli"
    )
'''
if block not in text:
    if text.count(anchor) != 1:
        raise SystemExit("Windows provider-router gate anchor missing")
    text = text.replace(anchor, block, 1)
windows.write_text(text, encoding="utf-8")

# Documentation changed above, so regenerate the docs inventory last.
docs = Path("docs")
manifest = docs / "MANIFEST.sha256"
entries = []
for path in sorted(docs.rglob("*")):
    if not path.is_file() or path == manifest:
        continue
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    entries.append(f"{digest}  {path.as_posix()}")
manifest.write_text("\n".join(entries) + "\n", encoding="utf-8")
