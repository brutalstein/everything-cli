# Repository Intelligence

## Objective

Maintain a reusable, commit-aware representation of a codebase that lets agents acquire the **smallest correct slice of repository knowledge** without repeatedly rediscovering structure.

Repository Intelligence is not a vector database and it is not a graph-only product. It is a **multi-view, provenance-carrying repository knowledge service** whose outputs are tied to an exact repository snapshot.

The Step-08 implementation established the first executable baseline: snapshot identity, bounded file inventory, lexical retrieval, Tree-sitter-backed syntax for the initial language set, symbols/dependency links, git/test views, content-hash reuse, abstention and stale-index refusal. Repository Intelligence 2.0 extends that baseline rather than replacing it.

---

## Settled target: Repository Intelligence 2.0

The long-term target is a **commit-scoped, multi-resolution, multi-language Repository Knowledge Fabric**.

Its purpose is to answer questions such as:

- where is this behavior implemented?
- what imports, calls, implements or configures it?
- what will probably break if this symbol changes?
- which tests, build targets, requirements and runtime failures are connected?
- what did this subsystem look like at the previous accepted snapshot?
- which facts are exact, which were semantically resolved, which are observed at runtime and which are only inferred?
- what is the cheapest source-grounded context sufficient for the current task?

No single representation is authoritative for relevance. Structural facts also MUST NOT be promoted to semantic truth merely because they appear in a graph.

## Why Tree-sitter exists in this architecture

Tree-sitter is the **universal syntax substrate**, not the complete semantic engine.

It provides:

- fast deterministic parsing;
- concrete syntax trees;
- incremental re-parsing after edits;
- useful trees even when source contains syntax errors;
- a broad ecosystem of language grammars.

Tree-sitter can tell AER that a file contains declarations, imports, calls or syntactic references when a grammar and extraction queries exist. By itself it generally cannot prove that a call such as `user.profile.display_name()` resolves to one exact definition across modules, packages, generics, inheritance and generated code.

Therefore AER MUST separate syntax extraction from semantic resolution.

---

## Language Capability Ladder

AER MUST represent support **per language and per capability**, never as one misleading `supported=true` flag.

```text
Tier 0  Universal text
        path / file metadata / exact text / lexical search / hashes

Tier 1  Syntax
        Tree-sitter AST / declarations / scopes / syntactic imports /
        call sites / structural chunks

Tier 2  Project resolution
        package manifests / module paths / build targets / dependency
        declarations / generated-source mappings / config topology

Tier 3  Precise semantics
        compiler, language-server or SCIP-derived definitions,
        references, implementations, inheritance, types and resolved calls

Tier 4  Dynamic evidence
        test coverage / stack traces / traces / profiles / runtime call
        observations / failure fingerprints
```

Rules:

1. Every safely readable source/text file gets Tier 0.
2. Every language with a vetted, pinned Tree-sitter grammar SHOULD get Tier 1.
3. Build/package adapters add Tier 2 independently of Tree-sitter.
4. Tier 3 is available only when the relevant semantic adapter can run reproducibly.
5. Missing higher tiers MUST degrade transparently to lower tiers.
6. The query engine MUST expose capability and confidence so an agent can distinguish exact semantic data from heuristic structure.
7. AER MUST never claim universal semantic understanding of “all programming languages.” The practical target is universal fallback plus progressively richer adapters.

### Language registry

Replace hard-coded language knowledge over time with a versioned registry inspired by mature language catalogs such as GitHub Linguist.

Each entry SHOULD describe:

```text
LanguageProfile {
  language_id
  aliases[]
  extensions[]
  filenames[]
  shebangs[]
  disambiguation_rules[]
  file_roles[]                  # programming, markup, data, prose, generated, vendored
  grammar_adapter?
  grammar_version?
  grammar_digest?
  extraction_query_version?
  project_resolvers[]
  semantic_adapters[]
  formatter/build/test_hints[]
  capability_tiers[]
}
```

Detection MAY use filenames, extensions, shebangs, modelines and deterministic heuristics. Ambiguous extensions MUST not silently choose a language when evidence is insufficient.

### Grammar distribution

The architecture SHOULD support two grammar classes:

- **native fast-path grammars** for high-frequency languages shipped and pinned with the release;
- **verified grammar packs** for the long tail, loaded through a constrained adapter boundary.

A grammar pack MUST be versioned, integrity-checked and included in cache identity. WebAssembly-backed grammars are a viable isolation/distribution option where their measured overhead is acceptable; they are not a blanket performance default. Grammar availability is an adapter concern, not a reason to weaken snapshot or supply-chain guarantees.

The language catalog SHOULD be able to grow to the breadth of the Tree-sitter ecosystem without forcing every grammar, compiler or language server into the core binary.

---

## Required repository views

AER maintains independently refreshable views which converge on the same snapshot identity:

1. **Lexical view** — files, identifiers, comments, exact strings, BM25/FTS/grep-style retrieval.
2. **Syntax view** — AST nodes, declarations, scopes, signatures, imports and call sites.
3. **Precise semantic view** — definition/reference/type/implementation/call resolution from compiler/LSP/SCIP-style adapters.
4. **Package/build view** — packages, modules, build targets, generated code, dependency manifests and test runners.
5. **Structural graph view** — normalized nodes/edges across code, build, docs and runtime evidence.
6. **Semantic representation view** — optional embeddings and compact role-aware representations for approximate retrieval.
7. **Git/temporal view** — commits, blame, rename continuity, co-change, recent edits and ownership where available.
8. **Test view** — tests, target associations, coverage and failing evidence.
9. **Runtime view** — stack traces, logs, traces, profiles and source anchors.
10. **Project-semantics view** — requirements, ADRs, architecture boundaries, owned modules and Proof Manifest links.
11. **Engineering-memory view** — verified facts, hypotheses, failure fingerprints and decisions whose validity is linked to repository evidence.

No one view owns truth for all queries.

---

## Canonical graph model

The graph is a normalized derived view over authoritative source/evidence, not an independent source of truth.

### Core node families

```text
Repository
RepoSnapshot
Directory
File
Module
Package
ExternalPackage
BuildTarget
GeneratedArtifact
Symbol
Type
Test
Requirement
Decision
ConfigKey
RouteOrEndpoint
DataEntity
Commit
RuntimeObservation
FailureFingerprint
MemoryFact
```

Domain adapters MAY introduce typed nodes without requiring the common schema to enumerate every framework concept.

### Core edge families

```text
contains
defines
imports
exports
resolves_to
references
calls
implements
inherits
aliases
depends_on
builds
generates
tests
covers
routes_to
reads_from
writes_to
configured_by
implements_requirement
constrained_by
changed_with
renamed_from
observed_in
failed_at
supports
contradicts
supersedes
validates
invalidates
```

### Provenance on every non-trivial edge

A relationship without provenance is unsafe for an autonomous engineering agent.

```text
EdgeEvidence {
  evidence_class        # extracted | semantic_resolved | observed | inferred
  confidence
  producer_id
  producer_version
  repo_snapshot
  source_artifact
  source_path?
  source_range?
  environment_fingerprint?
  created_at
  valid_from_snapshot
  valid_until_snapshot?
}
```

Interpretation:

- `extracted` — explicitly present in source/build metadata;
- `semantic_resolved` — resolved by compiler/LSP/SCIP or equivalent semantic authority;
- `observed` — witnessed in test/runtime evidence;
- `inferred` — heuristic or model-assisted relation.

`inferred` MUST never be returned as exact merely because confidence is high.

This borrows the useful “extracted vs inferred” distinction used by graph-oriented code tools while making the provenance model stricter and snapshot-aware.

---

## Stable identities and temporal continuity

Content hashes are excellent for artifact reuse but insufficient as the only symbol identity because code moves.

AER SHOULD maintain two identities:

1. **Snapshot identity** — exact location/content at one repository state.
2. **Logical symbol identity** — best-known continuity of the same logical entity across snapshots.

Logical identity can use language/package/module/FQN/kind/signature evidence plus rename/move history. Continuity is probabilistic unless a semantic index gives stronger evidence, so continuity mappings also carry provenance/confidence.

This enables queries such as “what changed in this subsystem?” without pretending that line numbers are permanent.

---

## Precise semantic adapters

AER SHOULD ingest rich code intelligence through adapters rather than re-implement every language compiler.

Preferred sources, in order of project suitability:

- language-native compiler APIs/indexers;
- Language Server Protocol clients when servers expose reliable project semantics;
- SCIP-compatible indexes for language-agnostic interchange;
- build-system metadata and package-manager graphs;
- syntax/heuristic fallback.

SCIP, LSP or a compiler index is an **input adapter**, not AER's internal authority. Imported records are normalized into AER's graph with snapshot, tool version and environment provenance.

AER MUST preserve the distinction between:

- syntactic call site;
- candidate target;
- compiler-resolved target;
- runtime-observed target.

---

## Build and package topology is first-class

Cross-language repositories often encode critical architecture outside source ASTs. Repository Intelligence 2.0 therefore indexes build/test/package topology explicitly.

Priority adapters SHOULD cover the dominant ecosystems encountered by AER benchmarks, for example:

- Cargo;
- npm/pnpm/yarn and TypeScript project references;
- Python `pyproject.toml`/lockfiles;
- Go modules/workspaces;
- Maven/Gradle;
- .NET solutions/projects;
- CMake/CTest and compile databases;
- Bazel where available;
- Docker/Compose;
- Kubernetes/Kustomize/Helm;
- Terraform/OpenTofu;
- SQL migration/schema topology;
- CI workflows.

This list is an adapter backlog, not a requirement that every installation start every tool.

Generated-code relationships SHOULD be represented explicitly so navigation can return the human-owned source rather than flooding context with generated implementations.

---

## Incremental indexing and freshness

Repository Intelligence MUST remain usable during continuous edits without repeatedly rebuilding the world.

### Artifact identity

Derived artifacts are keyed by at least:

```text
content_hash
language_profile_version
parser_or_adapter_id
parser_or_adapter_version
extraction_policy_version
relevant_project_environment_hash?
```

### Update frontier

On change:

1. compute the new workspace snapshot;
2. detect changed/added/deleted/renamed files;
3. reuse content-addressed artifacts for byte-identical files;
4. reparse only changed syntax artifacts;
5. identify dependency/build invalidation frontier;
6. re-resolve only affected semantic neighborhoods when safe;
7. invalidate stale graph edges and memory facts immediately;
8. refresh expensive embeddings/role summaries lazily;
9. publish the new snapshot only after required fast views are internally consistent.

Global re-index is justified only when a global dependency changes, such as parser/query version, language detection policy, build configuration with global impact, or index schema migration.

### Freshness epochs

Each view SHOULD expose:

```text
indexed_snapshot
producer_version
freshness_state = current | partially_current | stale | unavailable
```

Retrieval policy decides which freshness states are admissible for the task. Verification-critical decisions MUST fail closed rather than silently using stale semantic edges.

---

## Knowledge memory: Obsidian-like usability without Markdown as authority

AER should borrow the useful human model of linked notes:

- bidirectional links/backlinks;
- local graph neighborhoods;
- typed properties;
- readable rationale;
- easy inspection.

It SHOULD NOT use a folder of Markdown notes as the primary machine state.

Canonical engineering memory remains structured, evidence-backed state in the durable store. A read-only or explicitly regenerated **knowledge notebook export** MAY render selected symbols, decisions, requirements and verified facts as Markdown/Obsidian-compatible pages for human inspection.

Every exported page is a view. Editing an export MUST NOT silently mutate authoritative repository intelligence.

Backlinks are materialized automatically from graph edges, so an agent can cheaply ask:

```text
what references this?
what depends on this?
what changed with this?
what evidence supports this memory fact?
what invalidates this fact?
```

---

## Graph analytics

Graph algorithms are ranking aids, not semantic authority.

Useful derived signals include:

- inbound/outbound degree;
- shortest dependency path;
- strongly connected components;
- package/module communities;
- centrality/hub estimates;
- change-risk radius;
- test reachability;
- co-change clusters.

Community detection such as Leiden MAY help form subsystem summaries and navigation maps. A discovered community MUST NOT be treated as an architecture boundary unless source/build/requirement evidence supports it.

---

## Retrieval interface: answer with proof of relevance

Repository queries should return compact evidence, not graph dumps.

A result SHOULD include:

```text
candidate
why_relevant[]
capability_tier
edge_provenance[]
repo_snapshot
source_anchors[]
token_estimate
freshness
confidence
expansion_handles[]
```

Common graph-native operations:

```text
symbol(name)
definition(symbol)
references(symbol)
callers(symbol, depth)
callees(symbol, depth)
implementations(type)
imports(module)
dependents(target)
tests_for(target)
impact(change_set)
path_between(a, b)
history(entity)
backlinks(entity)
why_relevant(candidate, task)
```

Every traversal is bounded by depth/node/token policy.

---

## Token and latency economics

Repository Intelligence exists to reduce exploration work, not to create another huge context artifact.

Default query strategy:

```text
Task
  -> intent-aware query formulation
  -> exact path/symbol/lexical search
  -> bounded graph expansion
  -> build/git/test/runtime signals
  -> optional semantic/vector retrieval
  -> rank fusion
  -> task-specific reranking
  -> progressive disclosure
```

Cheap deterministic sources run first. Expensive semantic/vector/model operations are invoked only when expected information gain justifies their cost.

Precomputed role-aware summaries MAY be used as retrieval representations because compact representations can improve localization economics. They remain derived, hash-bound, non-authoritative artifacts; exact source spans are fetched before decision-critical edits.

A dedicated repository-explorer worker/model MAY later perform broad exploration and return only source-anchored paths/ranges/relevance reasons to the coding model. Its trajectory is not automatically injected into the solver context.

---

## Storage architecture

The local-first default remains deliberately simple until measurement proves otherwise:

- SQLite WAL for metadata, normalized graph tables, adjacency indexes and FTS;
- content-addressed object storage for larger derived artifacts;
- bounded in-memory hot adjacency/cache for the active snapshot;
- optional replaceable vector index;
- compressed cold payloads where benchmarked beneficially.

Do not introduce a graph database merely because the data is a graph. SQLite remains the baseline while its measured index-build, incremental-update and graph-query performance satisfy product targets.

For very large repositories, specialized adjacency/bitmap/index structures MAY be introduced behind the same Repository Intelligence API based on benchmarks.

---

## Safety and supply-chain rules

Language intelligence executes parsers and may invoke build/compiler tooling, so it is part of the execution threat surface.

- grammar/adapters MUST be pinned and integrity-verified;
- third-party parser packs are untrusted dependencies;
- compiler/LSP adapters run under Resource Governor and sandbox policy;
- indexing MUST respect ignored, generated, vendored, sensitive and oversized-file policies;
- external dependencies are represented without copying arbitrary third-party source into model context by default;
- malformed source/parser crashes cannot corrupt the authoritative store;
- partial index publication is transactional.

---

## Current baseline and migration

As of the Step-08 implementation, AER already has a valuable subset:

- snapshot-aware indexing and stale-index refusal;
- SQLite WAL storage;
- content-addressed syntax artifact reuse;
- lexical ranking;
- symbols and import/call/reference edges;
- git/co-change/test associations;
- Tree-sitter syntax adapters for Rust, Python, JavaScript, TypeScript and TSX;
- lexical fallback for other recognized text languages.

Repository Intelligence 2.0 MUST evolve this crate in place through versioned migrations/adapters. It MUST NOT create a second competing repository index.

The planned uplift is delivered as part of the existing 18-step architecture sequence, primarily with **Step 12 / Phase 6**, where long-horizon engineering memory requires temporal, invalidation-aware repository knowledge. Later language-specific architecture-health adapters can deepen the same graph without changing its authority model.

---

## Acceptance metrics

A future RI2 implementation is not accepted because it can draw a large graph.

Measure at least:

### Coverage

- files recognized by language;
- percentage of source bytes/files with Tier 1 syntax;
- percentage with Tier 2 project resolution;
- percentage eligible for Tier 3 precise semantics;
- unsupported/fallback rate;
- generated/vendor classification accuracy.

### Correctness

- definition/reference precision and recall;
- import/module resolution accuracy;
- resolved-call precision;
- implementation/inheritance accuracy;
- build/test dependency accuracy;
- stale-edge incidents;
- rename/continuity correctness;
- provenance correctness.

### Retrieval effectiveness

- relevant file Hit@K/MRR;
- relevant symbol/line recall;
- context yield per 1K tokens;
- post-seed exploration tokens/tool calls;
- abstention calibration;
- downstream verified solve rate.

### Performance

- cold full-index throughput;
- warm/content-reuse throughput;
- incremental update p50/p95;
- graph query p50/p95;
- time-to-first-relevant-symbol;
- peak RAM;
- persistent bytes per LOC/symbol;
- semantic-adapter startup/amortization.

### Adversarial fixtures

Include:

- syntax errors and partially edited files;
- ambiguous extensions;
- huge monorepos;
- mixed-language projects;
- generated code;
- vendored trees;
- rename/move;
- changed lock/build files;
- stale semantic indexes;
- unavailable/broken language servers;
- cyclic dependencies;
- submodules;
- dirty worktrees.

Benchmark against at least the current AER baseline, lexical-only, graph-only and embedding-only retrieval so hybrid complexity must earn its cost.

## Non-negotiable invariants

- repository intelligence is always snapshot-bound;
- no stale index is silently treated as current;
- every important graph relation has provenance;
- inferred edges do not impersonate exact semantic edges;
- unsupported semantic capability degrades explicitly;
- raw source/evidence remains retrievable behind summaries;
- retrieval remains bounded by token, latency and resource policy;
- language breadth cannot weaken sandbox, provenance or supply-chain requirements;
- a new storage/search technology is adopted only when AER-native benchmarks justify it.
