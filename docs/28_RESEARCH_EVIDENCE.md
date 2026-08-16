# Research Evidence and Design Rationale

**Research baseline:** 2026-08-16

This document records the external evidence that shaped the architecture. It is not a claim that every paper result will reproduce in AER; all imported ideas must be validated in the project's own eval suite.

## 1. Harness engineering / runtime separation

### OpenAI — Harness engineering: leveraging Codex in an agent-first world (2026-02-11)
https://openai.com/index/harness-engineering/

Key implications used here:

- reliable agents depend heavily on environment, feedback loops and repository legibility;
- agent-first repositories benefit from concise navigational context rather than huge instruction files;
- worktree-scoped observability can make logs/metrics directly usable by agents.

### OpenAI — Unrolling the Codex agent loop (2026-01-23)
https://openai.com/index/unrolling-the-codex-agent-loop/

### OpenAI — Unlocking the Codex harness / App Server (2026-02-04)
https://openai.com/index/unlocking-the-codex-harness/

Implication: reusable core/runtime separated from UI clients is a proven architecture pattern.

### Anthropic — Harness design for long-running application development (2026-03-24)
https://www.anthropic.com/engineering/harness-design-long-running-apps

### Anthropic — Effective harnesses for long-running agents (2025-11-26)
https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

Implication: context compaction alone does not solve long-horizon state; explicit progress/state handoff is necessary.

## 2. Specification before coding

### SpecFirst (2026-07)
https://arxiv.org/abs/2607.27167

Reports that a dedicated behavioral-specification stage before synthesis improved test pass rates across four models by 6.9–21.3 percentage points.

### SpecBench — specification-level reasoning (2026-05)
https://arxiv.org/abs/2605.30314

Shows that identifying deficiencies in real RFC-style specifications remains difficult even for frontier agents. This supports treating requirements review as a distinct capability.

## 3. Long-horizon difficulty

### RoadmapBench (2026-05)
https://arxiv.org/abs/2605.15846

115 real version-upgrade tasks; median changes span thousands of lines and dozens of files. Strong frontier systems remain far from complete reliability.

### SWE-Marathon (2026-06)
https://arxiv.org/abs/2606.07682

Highlights extreme token consumption and reward-hacking/integrity problems on ultra-long-horizon software work.

### DeepSWE (2026-07)
https://arxiv.org/abs/2607.07946

Uses original tasks and hand-written verifiers, emphasizing contamination-aware and developer-aligned evaluation.

## 4. Repository retrieval, intelligence and context

### Agent Retrieval Bench (2026-07)
https://arxiv.org/abs/2607.24882

Key result: no single retrieval family dominates. Dense embeddings, lexical methods and RepoMap-style structural context win on different subsets. RepoMap performs strongly under an 8K token budget.

Design implication: hybrid retrieval + budget-aware selection, not embedding-only RAG.

### CodeNib (2026-07)
https://arxiv.org/abs/2607.25431

Supports maintaining reusable lexical, dense and structural repository views tied to repository evolution.

### ContextSniper (2026-07)
https://arxiv.org/abs/2607.01916

Studies compact repair evidence to reduce repeated exploration/token waste.

### Compressing Code Context / SWEzze (2026-03)
https://arxiv.org/abs/2603.28119

Reports substantial token reduction with improved resolution under a source-aware compression strategy, motivating deliberate context compression rather than naive truncation.

### Problems of implicit context compression (2026-05)
https://arxiv.org/abs/2605.11051

Warns that compact latent/implicit representations that work for single-shot understanding may fail in multi-step coding agents. Supports source-faithful context and explicit provenance.

### On the Effectiveness of Context Compression for Repository-Level Tasks (2026-04)
https://arxiv.org/abs/2604.13725

Reports that several compression paradigms can reduce repository-context inference cost and, in selected settings, improve task metrics by filtering noise.

AER implication: compression is an optimization surface worth benchmarking, but opaque/latent compression does not replace source-addressable evidence in autonomous engineering decisions.

### Retrieval-Oriented Code Representations in Agentic Bug Localization (2026-07)
https://arxiv.org/abs/2607.11046

Finds that representation choice is a first-class retrieval variable. Role-aware summaries provided a strong cost/effectiveness trade-off and complementary representations plus post-retrieval ranking improved localization.

AER implication: maintain compact role-aware retrieval representations lazily, hash/provenance-bind them to source, and fetch exact source before decision-critical edits.

### FastContext — Training Efficient Repository Explorer for Coding Agents (2026-06)
https://arxiv.org/abs/2606.14066

Separates repository exploration from the main solver and returns concise source locations; reports lower coding-agent token use with improved end-to-end results in its evaluated settings.

AER implication: a future dedicated repository-explorer worker is useful only if it returns source-anchored localization rather than injecting its full exploratory transcript into the solver context.

### SWE-Explore (2026-06)
https://arxiv.org/abs/2606.07297

Evaluates repository exploration directly under a fixed line budget and highlights line-level coverage/ranking/context efficiency as important dimensions beyond file-level retrieval.

AER implication: RepoIntelBench/ContextBench must measure source-region coverage and token efficiency, not only final patch success.

### Code Isn't Memory: A Structural Codebase Index Inside a Coding Agent (2026-06)
https://arxiv.org/abs/2606.22417

Provides controlled evidence that a structural repository index can improve localization/resolve economics in a fixed agent harness for multi-file workloads.

AER implication: structural indexing deserves first-class status, but must still compete against lexical/semantic alternatives in AER-native evaluation.

### Repository Intelligence Graph (2026-01)
https://arxiv.org/abs/2601.10112

Studies deterministic, evidence-backed build/test structure for repository-aware agents and reports particularly useful effects on multilingual repositories.

AER implication: build targets, test topology and package relationships are first-class repository intelligence, not metadata to infer later from source chunks.

### Codebase-Memory (2026-03)
https://arxiv.org/abs/2603.27277
https://github.com/DeusData/codebase-memory-mcp

The paper evaluates a persistent Tree-sitter-based knowledge graph over 66 languages and reports substantially lower exploration token/tool use than file-by-file exploration with some answer-quality trade-off. The current open-source implementation advertises a substantially larger vendored grammar catalog and additional semantic-resolution tiers.

AER implication:

- persistent graph-native queries can remove repeated grep/read exploration;
- broad vendored grammar coverage is practical;
- parser-count claims and repository README benchmarks are not AER acceptance evidence;
- capability coverage must be reported by lexical/syntax/project-resolution/precise-semantic tier rather than one raw language count.

### Graphify (current open-source architecture)
https://github.com/Graphify-Labs/graphify

Graphify demonstrates useful design ideas: deterministic Tree-sitter code extraction, explicit `EXTRACTED` versus `INFERRED` relationships, bounded graph/path queries, community discovery and local inspectability.

AER implication: adopt explicit edge provenance and useful graph analytics, but do not adopt graph-only retrieval as a universal policy. AER retains lexical, structural, semantic, runtime, git and optional embedding views because the retrieval evidence remains task-dependent.

### Tree-sitter official architecture
https://tree-sitter.github.io/tree-sitter/

Tree-sitter is an incremental parsing system designed to build concrete syntax trees quickly, update them after edits and remain useful with syntax errors. Its ecosystem has many language grammars and supports distributable Wasm grammars.

AER implication: Tree-sitter is the universal **syntax substrate**, not a proof of exact program semantics. A grammar can identify declarations/import/call syntax; precise cross-file type/definition/call resolution requires richer project/compiler information.

### Language Server Protocol 3.18
https://microsoft.github.io/language-server-protocol/

LSP standardizes communication between development tools and language servers across many languages.

AER implication: use language servers as optional semantic adapters where they provide reproducible project intelligence; do not make one live LSP process the canonical store.

### SCIP / Sourcegraph precise code navigation
https://sourcegraph.com/docs/code-navigation/precise-code-navigation

SCIP is a language-agnostic code-intelligence interchange protocol. Sourcegraph distinguishes fast search-based navigation from precise navigation backed by language-specific indexes/compile-time information and falls back when precise indexes are unavailable.

AER implication: support SCIP/compiler/LSP ingestion for Tier-3 semantics while preserving deterministic lower-tier fallbacks. Imported indexes are normalized into AER snapshot/provenance semantics rather than becoming a second authority.

### Kythe
https://kythe.io/docs/kythe-overview.html
https://kythe.io/docs/schema/modules.html
https://kythe.io/docs/schema/indexing-generated-code.html

Kythe grew from large-scale multilingual semantic indexing and models language-agnostic cross-references, build/compiler metadata, imports and cross-language/generated-code relationships. Its design explicitly prefers incomplete information over incorrect information and encourages graceful degradation.

AER implication: normalize code intelligence into an extensible cross-reference graph, represent generated-source links, and fail transparently to weaker capabilities rather than fabricating semantic certainty.

### GitHub Linguist
https://github.com/github-linguist/linguist
https://github.com/github-linguist/linguist/blob/main/docs/how-linguist-works.md

Linguist combines extension/filename/shebang/modeline/heuristic/classifier evidence and identifies generated/vendored/data/prose roles.

AER implication: replace hard-coded language detection over time with a versioned language capability registry. Reuse the detection ideas/metadata model, not Linguist's Ruby implementation as a runtime dependency.

### Obsidian knowledge-network model
https://obsidian.md/help/links
https://obsidian.md/help/plugins/backlinks
https://obsidian.md/help/plugins/graph
https://obsidian.md/help/bases

Obsidian's internal links/backlinks/local graph and property views provide a strong human model for navigating a connected knowledge space while keeping data locally inspectable.

AER implication: provide bidirectional repository/memory backlinks and optionally regenerate an Obsidian-compatible human-readable knowledge notebook. Markdown is a view, not AER's canonical machine state.

### Repository Memory (2025-10)
https://arxiv.org/abs/2510.01003

Uses commit history, linked issues and functionality summaries as non-parametric repository memory to improve code localization.

AER implication: connect engineering memory to repository history/entities rather than restarting repository understanding from zero on every task.

### Is Agent Memory a Database? / Governed Evolving Memory (2026-05)
https://arxiv.org/abs/2605.26252

Frames long-term memory correctness as an evolving state problem involving ingestion, revision, forgetting and retrieval rather than append-only accumulation.

AER implication: repository-linked facts must be superseded/invalidated as code, dependencies, environment or evidence change. “Store more forever” is rejected.

### Anthropic — Effective context engineering for AI agents (2025-09)
https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents

Supports just-in-time context loading and progressive disclosure.

### Settled Repository Intelligence 2.0 synthesis

AER therefore rejects all four simplistic endpoints:

- vector-database-only repository RAG;
- graph-only repository intelligence;
- Tree-sitter-only “semantic” understanding;
- LLM-generated summaries as canonical repository memory.

The chosen architecture is a snapshot-bound hybrid knowledge fabric:

```text
universal lexical fallback
    + Tree-sitter syntax
    + package/build/test resolution
    + optional compiler/LSP/SCIP precise semantics
    + provenance-bearing structural graph
    + git/runtime/requirement evidence
    + governed temporal engineering memory
    + optional embeddings/role-aware representations
    -> token-aware progressive retrieval
```

All performance, language breadth and retrieval claims must be re-proven in RepoIntelBench/ContextBench on AER's own implementation.

## 5. Model routing and cost

### Agent-as-a-Router (2026-06)
https://arxiv.org/abs/2606.22902

Frames routing as Context → Action → Feedback → Context, using execution-grounded experience rather than one-shot task classification. Reports a 15.3% relative improvement from adding task-dimension performance statistics to a baseline router.

### SWE-Router (2026-07)
https://arxiv.org/abs/2607.00053

Uses a cheap exploratory partial trajectory before deciding whether escalation is worthwhile. Motivates scout-then-route.

### Triage / cost-effective routing (2026-04)
https://arxiv.org/abs/2604.07494

Motivates using the cheapest tier that can pass the same verification standard.

### SuperScout / scouting before routing (2026-08)
https://arxiv.org/abs/2608.04804

Adds repository scouting and sandbox-verified handoff before choosing an expensive coding agent; strengthens the case that task text alone is insufficient for routing.

## 6. Multi-agent coordination

### CAID — Effective Strategies for Asynchronous Software Engineering Agents (2026-03, updated 2026-07)
https://arxiv.org/abs/2603.21489

Uses dependency-aware decomposition, isolated git branches/worktrees, branch-and-merge and executable verification. Reports strong gains on selected long-horizon tasks over single-agent baselines.

Design implication: use worktrees and dependency-aware parallelism, but only when task structure supports it.

### Anthropic — How we built our multi-agent research system (2025-06)
https://www.anthropic.com/engineering/multi-agent-research-system

Highlights coordination/token costs and the importance of task decomposition.

## 7. Handoff / long-horizon continuity

The architecture incorporates a general result emerging from takeover/handoff research: compact transfer of action/state/evidence is more useful than forwarding entire transcripts. AER therefore defines a typed Handoff ABI and preserves raw artifacts by reference.

This area should be continuously re-evaluated because the 2026 literature is rapidly evolving.

## 8. Verification and reward hacking

### Verification Horizon (2026-06)
https://arxiv.org/abs/2606.26300

Argues that verification quality has competing dimensions (scalability, faithfulness, robustness), that no single verifier is a permanent silver bullet, and that verification must co-evolve with generators.

### Measuring Reward Hacking / SpecBench (2026-05)
https://arxiv.org/abs/2605.21384

Shows visible-test saturation can coexist with held-out failures and that the gap worsens with task length.

Design implication: separate generator/verifier authority; use held-out/integrity signals and multi-layer verification.

## 9. Long-horizon code quality

### SlopCodeBench (2026-03)
https://arxiv.org/abs/2603.24755

Reports rising structural erosion and verbosity across iterative agent trajectories; simple quality prompting does not stop degradation.

Design implication: architecture health must be measured continuously and enforced by the runtime.

## 10. Security / sandboxing

### Anthropic — How we contain Claude across products (2026-05)
https://www.anthropic.com/engineering/how-we-contain-claude

### Anthropic — Claude Code sandboxing (2025-10)
https://www.anthropic.com/engineering/claude-code-sandboxing

Supports filesystem + network isolation and highlights approval fatigue.

### OpenAI — Agents SDK native sandboxing (2026-04)
https://openai.com/index/the-next-evolution-of-the-agents-sdk/

### OpenAI — Windows Codex sandbox (2026-05)
https://openai.com/index/building-codex-windows-sandbox/

### Docker — Rootless mode / AI sandbox isolation
https://docs.docker.com/engine/security/rootless/
https://docs.docker.com/ai/sandboxes/security/isolation/

### Firecracker
https://firecracker-microvm.github.io/

### OWASP Top 10 for Agentic Applications 2026
https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/

### NIST — Identity and Authority of Software Agents (2026)
https://www.nccoe.nist.gov/news-insights/new-concept-paper-identity-and-authority-software-agents

Design implication: model text is not authority; autonomy must be constrained by least-privilege capability boundaries.

## 11. External protocols

### MCP 2026-07-28 specification
https://modelcontextprotocol.io/specification/2026-07-28

Current release introduces a stateless core and official Tasks extension. Security guidance emphasizes per-request authorization and OAuth-oriented practices.

### A2A v1.0
https://a2a-protocol.org/latest/

Useful for interoperability between independent/opaque agent systems. AER keeps it at the external boundary rather than using it as internal state.

### OpenTelemetry GenAI
https://opentelemetry.io/docs/specs/semconv/gen-ai/

Provides a standard basis for model/tool/token tracing; AER extends it with orchestration-specific attributes.

## 12. Self-improvement

### OpenAI — Agent improvement loop (2026-05)
https://developers.openai.com/cookbook/examples/agents_sdk/agent_improvement_loop

Supports trace/eval-based iterative improvement rather than prompt changes based only on intuition.

### Self-Evolving Coding Agents (2026-08)
https://arxiv.org/abs/2608.03392

Maps an emerging research direction where memory, tools, harness and collaboration structure evolve from software-engineering experience.

Design implication: evolution is promising, but AER separates candidate generation from evaluation/promotion.

## Evidence-strength policy

The design uses sources differently:

- official protocol/spec/security docs define interoperability/security facts;
- peer-reviewed/preprint research motivates hypotheses and benchmark-informed choices;
- company/open-source engineering reports provide implementation evidence but are not treated as universal scientific proof;
- project README benchmark claims are references to reproduce, not acceptance evidence.

Every high-impact research-derived policy should ultimately be validated by AER-native evals.
