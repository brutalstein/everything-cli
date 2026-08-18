from __future__ import annotations

import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

SECTIONS: dict[str, tuple[str, str]] = {
    "docs/06_REPOSITORY_INTELLIGENCE.md": (
        "<!-- context-economy-v2-ri2 -->",
        r'''

<!-- context-economy-v2-ri2 -->
## Context Economy V2: hierarchical RI2 capsules

RI2 remains the only repository intelligence index and source-of-truth localization fabric. Context Economy V2 adds **derived repository capsules**, not a second index, database, memory, or retrieval system. `RepositoryIndex::repository_capsule` projects already-indexed RI2 facts into bounded navigation records at repository, package, directory, file, and symbol scope.

A capsule may expose only compact deterministic facts that RI2 can ground: canonical scope identity, a deterministic primary-role label, key symbols/signatures, dependency and dependent edges, test links, build targets, source anchors, content hashes, capability tier, snapshot freshness, and producer version. Capsule identity is stable for the same logical scope; truth remains snapshot-bound through the capsule snapshot id, source anchors, hashes, and freshness. A stale snapshot is never made current by a capsule.

Capsules are for **narrowing and navigation**. They never replace exact source for an edit, verification-critical decision, or exact-definition demand. For large repositories the intended narrowing sequence is repository/workspace or package -> directory/module -> file -> symbol -> exact source. Repository size should increase local indexing/query work much more than provider-visible context size. The CI-safe synthetic large-repository gate currently plants thousands of unrelated files and requires an exact-definition task to remain localized to one exact source item without invoking broad lexical or structural retrieval.

Capsule production is deterministic and bounded by explicit per-field limits. Optional future model-written role descriptions, if introduced, must remain non-authoritative, content-hash-bound, source-anchored, lazy, and invalidated with their source scope. No such model-written summaries are authoritative in the current implementation.
''',
    ),
    "docs/07_CONTEXT_ECONOMY_ENGINE.md": (
        "<!-- context-economy-v2-engine -->",
        r'''

<!-- context-economy-v2-engine -->
## Context Economy V2: evidence sufficiency, assembly, and offline quality gates

Context Economy now compiles a typed `EvidenceDemand` set before selecting provider-visible repository evidence. A demand records the information target, required tier/provenance, minimum coverage, expansion policy, importance, and whether it is verification-critical. Current demand classes cover exact definitions, requirement/implementation context, runtime evidence, edit targets, test context, supporting context, and change impact. The representation is deliberately capability- and task-oriented rather than provider- or benchmark-oriented.

Retrieval is progressive. The engine first establishes the verified RI2 snapshot without forcing a lexical query. Exact symbol/semantic/runtime evidence can enter the `Exact` stage and terminate discovery when sufficient. Lexical localization runs only for objective demands that require it. Bounded structural/impact expansion runs only when a remaining demand actually requires neighborhood coverage. Exact-definition ambiguity and missing mandatory evidence continue to fail closed.

### Budget is a ceiling

`input_token_budget` is a hard maximum, never a target. Selection repeatedly admits only a candidate that contributes marginal coverage to an unsatisfied demand. Once every demand is satisfied, selection stops immediately even when budget remains. The old spare-budget `Structural -> SourceSpan -> Expanded` upgrade sweep is removed. Tier escalation now requires a demand whose minimum tier justifies the richer representation, and the selected item's reason records the demand relationship.

A regression invariant is explicit: with identical task and evidence, a 6,144-unit ceiling and a 12,288-unit ceiling must produce the same model-visible semantic payload when 6,144 is already sufficient. Unused budget is a successful outcome.

### Semantic selection is separate from provider assembly

Context Economy decides **which semantic facts are required**. `ContextAssemblyPlanner` decides how those already-selected facts are legally ordered for a transport. Provider-neutral `ContextSegment` records semantic role, trust class, reuse scope, volatility, content hash, deterministic context-unit estimate, source references, and rendered bytes. Audit-only source references/hashes are not rendered merely to improve cache accounting.

Trust is stronger than cache economics. Repository/task evidence remains `UntrustedData`; it cannot be promoted into `SystemAuthority`. Provider cache capability can alter legal ordering or breakpoint geometry but cannot add/remove semantic requirements. For common-prefix transports stable snapshot evidence precedes iteration-dynamic material inside the untrusted layer, while decision-critical evidence remains close to the objective/output contract according to the assembly role order. The current delegated Claude CLI is modeled only as an implicit common-prefix transport because AER does not control independent per-file cache objects or breakpoints there. Codex/Gemini delegated transports conservatively use no-cache geometry until equivalent behavior is established.

### Working set and deltas

Long-running engineering loops use an ephemeral `TaskWorkingSet` projection over existing Engineering State/Handoff facts. It carries edit targets, relevant symbols, verified facts, architecture constraints, latest failures, tests, changed files, unresolved hypotheses, and stable evidence identities. `ContextDelta` reports added, changed, removed, and invalidated evidence. A content-hash change invalidates immediately; unrelated metadata churn does not change semantic evidence identity. This is not a second persistence or memory subsystem.

### Deterministic quality accounting

Offline gates use deterministic context-unit estimates and provider-visible byte counts; these are **not provider tokenizer counts or provider cost measurements**. Current gates measure mandatory-like versus optional context, selected source lines, redundant selected lines, overlapping span pairs, retrieval stages, unnecessary stages, tier escalation, provider-visible bytes, large-repository boundedness, budget invariance, cache assembly invariants, freshness, and compact edit output size. Provider economics claims still require real provider telemetry.
''',
    ),
    "docs/09_ADAPTIVE_MODEL_ROUTER_AND_BUDGETS.md": (
        "<!-- context-economy-v2-cognitive-routing -->",
        r'''

<!-- context-economy-v2-cognitive-routing -->
## Context Economy V2: cognitive work roles and deterministic-first routing

The routing layer now has provider-neutral cognitive work roles: `Deterministic`, `Scout`, `Locator`, `RepositoryExplorer`, `FailureAnalyst`, `EvidenceCompressor`, `Planner`, `Coder`, and `SemanticReviewer`. These are work capabilities, not model names. They do not grant execution authority and they do not require every role to invoke an LLM.

The default policy is deterministic-first: exact retrieval, graph traversal, freshness/state tracking, validation, context assembly, and cheap classification remain in AER's deterministic control plane whenever adequate. Bounded ambiguous scouting/localization may later be assigned to a low-cost or local provider. Planning/coding/semantic review must not be silently downgraded solely for price when the requested reasoning capability is not met. The deterministic verifier remains the acceptance authority.

Provider/model eligibility continues to be resolved from capabilities and policy. Cache geometry is also a transport capability, not a model-name branch. An OpenAI-compatible multi-provider gateway can therefore be added later as transport while AER retains requested capability, task role, security constraints, budgets, escalation, verification, and acceptance. An external router's `auto` policy is not AER's control plane.
''',
    ),
    "docs/12_HANDOFF_ABI_AND_COGNITIVE_ADAPTERS.md": (
        "<!-- context-economy-v2-handoff -->",
        r'''

<!-- context-economy-v2-handoff -->
## Context Economy V2: working-set deltas and compact coder packets

The Handoff ABI remains the cross-boundary carrier for engineering state; Context Economy V2 does not introduce transcript memory. A task may derive an ephemeral `TaskWorkingSet` from existing Engineering State/Handoff facts and compare consecutive projections with `ContextDelta { added, changed, removed, invalidated }`. Evidence retains a stable logical identity while its content hash is unchanged. Changed or removed evidence is invalidated immediately rather than reused for cache savings.

Coding-worker packets are intentionally machine-oriented. They should contain the smallest verified edit evidence needed for the current action, the user objective, the compact output contract, and only necessary blocked/new facts. The worker is not asked to echo repository context, old test logs, or unchanged source.

Single-Agent Runtime 0.1 now uses the reusable compact edit ABI instead of whole-file `{ path, content }` replacement. `replace_range` binds a path to an exact base-file SHA-256 and exact segment SHA-256 plus original line coordinates; `create_file` is explicit; `delete_file` requires exact base identity. All operations are preflighted before mutation. Stale bases/ranges, overlapping ranges, conflicting operations, path traversal, protected `.git`/`.aer` paths, symlink targets, non-regular targets, and configured operation/byte-limit violations fail closed. Application is deterministic, result hashes are recorded, and already-applied mutations are rolled back if a later path mutation fails.

The current tool-free runtime supplies exact edit evidence only for bounded verifier-declared edit targets, including base SHA-256 and per-line segment hashes for existing UTF-8 files. The provider may mutate only those evidence-backed paths. This keeps source truth in AER and lets a coding model emit changed text rather than entire files. It does not expose shell execution or claim completion of the future strong sandbox/tool-loop phase.
''',
    ),
    "docs/46_PROVIDER_CONTEXT_ECONOMICS_BENCHMARK.md": (
        "<!-- context-economy-v2-offline-economics-note -->",
        r'''

<!-- context-economy-v2-offline-economics-note -->
## Context Economy V2 validation note

The Context Economy V2 engineering pass intentionally adds **no new live provider-economics result**. Provider quota was treated as a hard resource constraint, so implementation validation uses deterministic/offline repository, context, cache-geometry, freshness, and edit-output gates. The resulting context-unit estimates and provider-visible byte counts must not be relabeled as provider tokenizer counts, cache-hit rates, dollars, or latency.

The historical live evidence in this document remains unchanged. In particular, the earlier pilot's cache-write/cache-read observations remain evidence about that transport/session, not a target fitted into production selection logic. A later live economics rerun should be performed only when quota is intentionally available and should measure the then-current provider transport with real usage telemetry.
''',
    ),
    "docs/48_CLAUDE_CODE_PARITY_BENCHMARK.md": (
        "<!-- context-economy-v2-parity-offline-note -->",
        r'''

<!-- context-economy-v2-parity-offline-note -->
## Context Economy V2 offline parity diagnostics

The historical Claude Code parity pilot and its measured answers/usage remain unchanged. This Context Economy V2 pass does not fabricate replacement parity results and does not run the live parity suite.

The parity harness continues to support its zero-provider-call path: without `--live`, it compiles the benchmark tasks' Context Packs and reports selected evidence/deterministic estimates while the provider-call count remains zero. Context Economy V2 uses that mode for structural diagnostics only. Those diagnostics can establish mandatory evidence coverage, selected tiers, stable/dynamic composition, redundancy, and context boundedness; they cannot establish Claude answer quality, real tokenizer counts, cache economics, latency, or dollar cost.
''',
    ),
    "STATUS.md": (
        "<!-- context-economy-v2-status -->",
        r'''

<!-- context-economy-v2-status -->
## Context Economy V2 engineering pass — current branch truth

On `feat/context-economy-v2`, AER now has demand-driven evidence sufficiency, progressive retrieval, budget-ceiling semantics, derived hierarchical RI2 capsules, provider-neutral context assembly/cache geometry, provider-neutral cognitive work roles, task working-set deltas, and a hash-bound compact edit ABI wired into Single-Agent Runtime 0.1.

Deterministic regressions cover exact-definition early stopping, budget invariance after sufficiency, preservation of test/implementation evidence, stale-snapshot rejection, trust/cache assembly boundaries, compact-edit stale/overlap/path/symlink failure modes, deterministic replay, sparse-edit output economy, and a CI-safe synthetic repository with thousands of unrelated files whose exact localized task remains bounded.

No live provider benchmark or provider-economics loop was run for this pass. Live provider calls used by this pass: **0**. Therefore no new claim is made about real provider tokenizer counts, cache-hit economics, latency, dollar cost, or parity answer quality. The prior live parity/provider evidence remains historical evidence and is not rewritten.

The pass is complete only when the canonical Linux foundation gates, canonical isolated Windows verifier, documentation integrity, and the feature PR checks are green. Until then this section records implemented branch behavior, not merged-main truth.
''',
    ),
}


def append_once(relative: str, marker: str, section: str) -> None:
    path = ROOT / relative
    text = path.read_text(encoding="utf-8")
    if marker in text:
        raise SystemExit(f"{relative}: Context Economy V2 marker already exists")
    if not text.endswith("\n"):
        text += "\n"
    path.write_text(text + section.lstrip("\n"), encoding="utf-8")


def refresh_manifest() -> None:
    docs = ROOT / "docs"
    entries: list[str] = []
    for path in sorted(p for p in docs.rglob("*") if p.is_file() and p.name != "MANIFEST.sha256"):
        relative = path.relative_to(ROOT).as_posix()
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        entries.append(f"{digest}  {relative}")
    (docs / "MANIFEST.sha256").write_text("\n".join(entries) + "\n", encoding="utf-8")


for relative, (marker, section) in SECTIONS.items():
    append_once(relative, marker, section)
refresh_manifest()
print(f"updated {len(SECTIONS)} ownership documents and refreshed docs/MANIFEST.sha256")
