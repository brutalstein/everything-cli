from __future__ import annotations

from pathlib import Path
import hashlib

ROOT = Path(__file__).resolve().parents[1]


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"missing anchor for {label}")
    return text.replace(old, new, 1)


# STATUS.md — preserve the historical ledgers and update only the provider gate/current-state surfaces.
status_path = ROOT / "STATUS.md"
status = status_path.read_text(encoding="utf-8")
status = replace_once(status, "**Last updated:** 2026-08-16", "**Last updated:** 2026-08-17", "status date")
status = replace_once(
    status,
    "**Repository-side state:** IMPLEMENTED — authoritative Linux/Windows CI pending  \n**Provider branch:** `agent/provider-runtime-productization`  \n**Provider gate:** delegated Codex/Claude/Gemini auth transports + Architecture Context Capsule + permission controller + AER ToolBroker + real-model smoke surface  \n**Next step:** 14 — Architecture Health Controller — BLOCKED until provider productization target-machine live smoke closes",
    "**Repository-side state:** MERGED + CI GREEN — real Claude transport reproduced on target Windows; provider gate remains OPEN on isolation/context-economy/telemetry acceptance  \n**Current main:** `1ba6206600d10a44aa6d5114a3510ad03806d205` before this documentation closeout; post-merge `foundation-ci` `31975867579` SUCCESS  \n**Provider implementation:** PR #6 merged; Windows fixture repair PR #7 merged; Claude smoke turn-limit repair PR #8 merged  \n**Provider gate:** delegated Codex/Claude/Gemini auth transports + Architecture Context Capsule + permission controller + AER ToolBroker + real-model smoke surface; live testing exposed provider-local hook contamination and excessive static context  \n**Next step:** Provider isolation + compact contextual bootstrap + complete usage telemetry; Step 14 remains BLOCKED until these acceptance gaps close",
    "status current provider state",
)
status = replace_once(
    status,
    "**State:** IMPLEMENTED — AUTHORITATIVE REPOSITORY CI + TARGET LIVE SMOKE PENDING",
    "**State:** IMPLEMENTED + MERGED + CI GREEN — TARGET CLAUDE TRANSPORT LIVE, PRODUCT ACCEPTANCE STILL OPEN",
    "provider gate state",
)
status = replace_once(
    status,
    "| Permanent Linux provider/permission/tool gate | PENDING | final authoritative PR CI. |\n| Canonical Windows provider/permission/tool gate | PENDING | final authoritative PR CI. |\n| Temporary provider repair/finalization workflows removed | PENDING | required before final PR acceptance. |\n| Target Windows canonical verifier after merge | PENDING | user reproduction required. |\n| Target-machine delegated OAuth + real model smoke | PENDING | at least one provider required to close this gate; all available requested providers should be exercised. |",
    "| Permanent Linux provider/permission/tool gate | PASS | PR #6 `foundation-ci` `31971591717`; post-merge main CI `31972088680`. |\n| Canonical Windows provider/permission/tool gate | PASS | PR #6 Windows CI; post-merge main CI `31972088680`. |\n| Windows ToolBroker fixture cleanup robustness | PASS | target reproduction exposed teardown locking; PR #7 fixed collision-safe fixture identity + bounded transient-lock cleanup; PR/main Windows CI passed. |\n| Claude delegated smoke `--max-turns 1` incompatibility | PASS | target live call exposed `terminal_reason=max_turns`; PR #8 removed the redundant cap; final `main` `1ba6206…` CI `31975867579` passed. |\n| Temporary provider repair/finalization workflows removed | PASS | permanent workflow tree remains `.github/workflows/ci.yml` only. |\n| Target Windows provider discovery | PARTIAL | Claude `2.1.233` authenticated on Claude.ai Pro; Codex PATH resolves to an invalid Win32 shim (`os error 193`); Gemini CLI unavailable. |\n| Target-machine real Claude model transport | PASS | authenticated Claude print-mode call returned machine JSON and normalized final output; latest trace duration `43321 ms`, output tokens reported `2576`, raw event count `1`. |\n| Architecture context reaches real model | PARTIAL | capsule digest/source list was transmitted and the response referenced AER/`AGENTS.md` constraints, but provider-local DeepWork behavior contaminated the answer. |\n| Provider-local behavioral isolation | FAIL / OPEN | global Claude Code hook/skill behavior reached the delegated subprocess and displaced the requested AER Q&A response; vendor auth may be inherited, vendor-local behavioral policy may not silently become AER authority. |\n| Context economy for provider bootstrap | FAIL / OPEN | prior raw Claude invocation reported `32563` cache-creation input tokens for the static architecture payload; production bootstrap must become compact invariant core + task-relevant RI2/context retrieval. |\n| Complete provider usage telemetry | FAIL / OPEN | normalized trace currently under-reports effective input because cache-creation/cache-read/thinking/cost dimensions are not preserved separately. |\n| Target-machine delegated OAuth + real model smoke acceptance | PARTIAL | OAuth + inference + parsing work for Claude, but relevance/isolation/context-economy/telemetry acceptance is not yet satisfied. |",
    "provider acceptance ledger",
)
status = replace_once(
    status,
    "Do **not** start Step 14 until the clean production tree passes authoritative Linux + canonical Windows CI, is merged to `main`, the target Windows checkout passes the canonical verifier, and at least one delegated provider completes a real authenticated model smoke showing the final input/output receipt and architecture-context identity. Provider-specific local unavailability should be recorded explicitly rather than fabricated.",
    "Do **not** start Step 14 until the clean production tree passes authoritative Linux + canonical Windows CI, is merged to `main`, the target Windows checkout remains reproducible, and at least one delegated provider completes a real authenticated model call that is **AER-controlled rather than provider-local-policy controlled**. Closure now additionally requires: (1) provider-local hooks/skills/config cannot silently redirect AER behavior; (2) the stable architecture bootstrap is compact and task-specific context is retrieved through RI2/Context Economy rather than repeatedly shipping a ~30k-token static payload; and (3) usage receipts preserve effective input, cache creation/read, output, thinking when reported, cost/model identity and latency. Provider-specific local unavailability is recorded explicitly rather than fabricated.",
    "provider exit condition",
)
status_path.write_text(status, encoding="utf-8")


# DEVELOPMENT_PLAN.md — strengthen the already non-numbered provider gate; roadmap numbering is unchanged.
plan_path = ROOT / "DEVELOPMENT_PLAN.md"
plan = plan_path.read_text(encoding="utf-8")
plan = replace_once(
    plan,
    "- one provider-neutral, source-hashed, bounded Architecture Context Capsule so every model receives the same stable product/architecture identity before task-specific context;\n- a live read-only provider smoke call with inspectable input/output/usage/context identity;",
    "- one provider-neutral, source-hashed, bounded architecture bootstrap so every model receives the same stable product/architecture identity before task-specific context; after live validation, this MUST converge from large static document slices to a compact invariant/constitutional core plus task-relevant RI2/Context Economy retrieval;\n- a live read-only provider smoke call with inspectable input/output/usage/context identity; provider authentication may be inherited, but provider-local hooks, skills, memory or behavioral configuration MUST NOT silently become AER control-plane policy;",
    "development plan context/isolation",
)
plan = replace_once(
    plan,
    "- deterministic CI remains free of live credentials and paid model calls; at least one target-machine delegated provider login + real model smoke must be reproduced before this gate closes.",
    "- usage telemetry must preserve provider-reported uncached input, cache creation/read, output, thinking/reasoning when available, resolved model, latency and cost without exposing secrets or hidden reasoning;\n- deterministic CI remains free of live credentials and paid model calls; at least one target-machine delegated provider login + real model smoke must be reproduced **with relevant AER-directed output and no provider-local policy contamination** before this gate closes.",
    "development plan telemetry",
)
plan_path.write_text(plan, encoding="utf-8")


# Normative provider specification — incorporate live target-machine findings.
doc_path = ROOT / "docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md"
doc = doc_path.read_text(encoding="utf-8")
doc = replace_once(
    doc,
    "All vendor subprocesses use fixed AER-constructed argv. Model output never supplies the executable or provider-control flags.\n\n---",
    "All vendor subprocesses use fixed AER-constructed argv. Model output never supplies the executable or provider-control flags.\n\n### 3.1 Provider-local behavior isolation\n\nDelegated authentication and delegated behavior are different trust decisions. AER MAY reuse a vendor-owned authenticated session, but it MUST NOT silently inherit user-level provider hooks, skills, memory, project instructions, permission bypasses or other behavioral configuration as control-plane authority.\n\nA real provider call is acceptable only when the effective behavior is attributable to the AER request envelope and AER policy. If a headless CLI cannot provide that isolation reliably, the adapter MUST move to an isolated provider configuration/profile or the provider's structured SDK/control protocol before agentic execution is accepted. A provider-local hook blocking or redirecting a read-only AER smoke is a typed isolation failure, not a successful model answer.\n\n---",
    "provider isolation subsection",
)
doc = replace_once(
    doc,
    "The authoritative model bootstrap is an AER-generated **Architecture Context Capsule**.\n\nInitial capsule sources:",
    "The authoritative model bootstrap is an AER-generated **Architecture Context Capsule**. The first implementation intentionally used bounded document slices to prove cross-provider identity transmission. Live target-machine evidence showed that this bootstrap can still be much larger than a production default should be, so the next architecture-complete uplift is a compact stable invariant/constitutional core plus task-relevant RI2/Context Economy retrieval.\n\nInitial proof-slice capsule sources:",
    "context capsule evolution",
)
doc = replace_once(
    doc,
    "A model call records the exact capsule digest. Task-specific Engineering IR, repository evidence, Handoff ABI and retrieved code are added later through the Context Economy Engine rather than by dumping the whole repository into every request.",
    "A model call records the exact bootstrap/capsule digest. Task-specific Engineering IR, repository evidence, Handoff ABI and retrieved code are added through the Context Economy Engine rather than by dumping the whole repository into every request. The production path MUST optimize measured relevant-information yield per token; repeatedly sending tens of thousands of static architecture tokens is not an acceptable steady-state design merely because the provider can cache them.",
    "context economy rule",
)
doc = replace_once(
    doc,
    "input/output token usage when provider reports it\nduration\nraw structured-event count/provider request id when available",
    "uncached input token usage when provider reports it\ncache-creation and cache-read token usage when provider reports it\noutput token usage and thinking/reasoning token usage when separately reported\nresolved model identity and provider-reported cost when available\nduration\nraw structured-event count/provider request id when available",
    "usage receipt fields",
)
doc = replace_once(
    doc,
    "5. machine output parses into a final answer and usage receipt.",
    "5. machine output parses into a final answer and usage receipt;\n6. the answer is responsive to the AER request rather than redirected by provider-local hooks/skills/configuration;\n7. provider-reported cache/input/output/model/cost dimensions needed for truthful accounting are preserved when available.",
    "live smoke criteria",
)
doc = replace_once(
    doc,
    "- no reading all architecture docs into every prompt;",
    "- no reading all architecture docs into every prompt; the stable bootstrap should be compact and task-specific detail should come from RI2/Context Economy retrieval;",
    "performance context rule",
)
doc = replace_once(
    doc,
    "- Provider-native tools cannot bypass the AER Tool ABI in accepted agentic execution.",
    "- Provider-native tools cannot bypass the AER Tool ABI in accepted agentic execution.\n- Provider-local hooks, skills, memory and behavioral configuration cannot silently become AER authority merely because AER reuses the vendor's authenticated session.",
    "security provider local rule",
)
doc = replace_once(
    doc,
    "At every stage the router, Engineering IR, context/provenance, tool authority and verification system remain provider-neutral.\n\n---\n\n## 12. Primary-source basis",
    "At every stage the router, Engineering IR, context/provenance, tool authority and verification system remain provider-neutral.\n\n### Immediate post-smoke uplift\n\nBefore this productization gate closes, implement and verify three corrections exposed by the first target-Windows Claude calls:\n\n1. **behavior isolation:** authenticated vendor sessions may be reused, but user/global provider policy must not redirect AER requests;\n2. **context economy:** replace the large static architecture payload with a compact invariant core plus task-relevant RI2/Context Economy material under measured token budgets;\n3. **truthful telemetry:** normalize uncached input, cache creation/read, output, thinking when reported, resolved model, cost and latency rather than collapsing effective usage into an incomplete token pair.\n\n---\n\n## 12. Primary-source basis",
    "immediate uplift",
)
doc += "\n---\n\n## 13. 2026-08-17 target-Windows live validation record\n\nThe first real Claude integration sequence established the following facts without closing the productization gate:\n\n- Claude Code `2.1.233` was discovered as authenticated through a Claude.ai Pro session.\n- A first Opus 5 call reached the provider and reported approximately `21861 ms` API duration, `32563` cache-creation input tokens, `1536` output tokens and `156` thinking tokens, but AER's redundant `--max-turns 1` caused Claude Code to terminate with `terminal_reason=max_turns`; PR #8 removed that cap.\n- After the repair, a target-Windows Claude print-mode call completed and AER parsed a final machine-readable answer; the observed AER trace reported `43321 ms`, `2576` output tokens and one raw event.\n- The answer was not acceptable: user/global Claude Code DeepWork behavior redirected the explanatory AER prompt into its own gate response. This proves transport/authentication/inference/parsing but fails provider-behavior isolation and response-relevance acceptance.\n- Codex discovery currently resolves a local PATH entry that fails as a Win32 executable (`os error 193`); Gemini CLI was unavailable on that target machine. These are local provider-availability findings, not fabricated provider failures.\n\nTherefore the gate remains OPEN. Step 14 stays blocked until behavior isolation, compact contextual bootstrap and complete usage telemetry are implemented and a clean target-machine real-model call satisfies the normative acceptance criteria above.\n"
doc_path.write_text(doc, encoding="utf-8")


# Regenerate the docs manifest deterministically after semantic doc changes.
docs = ROOT / "docs"
entries: list[str] = []
for path in sorted(p for p in docs.rglob("*") if p.is_file() and p.name != "MANIFEST.sha256"):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    rel = path.relative_to(ROOT).as_posix()
    entries.append(f"{digest}  {rel}")
(docs / "MANIFEST.sha256").write_text("\n".join(entries) + "\n", encoding="utf-8")
