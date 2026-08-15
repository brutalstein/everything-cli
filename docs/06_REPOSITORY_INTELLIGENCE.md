# Repository Intelligence

## Objective

Maintain a reusable, commit-aware representation of a codebase that allows agents to acquire relevant context without repeatedly rediscovering repository structure.

Repository intelligence is not a vector database. It is a **multi-view code knowledge service**.

## Required views

AER SHOULD maintain the following independently refreshable views:

1. **Lexical view** — files, identifiers, comments, exact strings, BM25/grep-style retrieval.
2. **Syntactic view** — AST nodes, declarations, references, imports, signatures.
3. **Semantic embedding view** — code/document chunks for approximate semantic retrieval.
4. **Symbol/dependency view** — definition-reference, call/import/module relationships.
5. **Git view** — commit history, co-change, blame, recent edits, ownership where available.
6. **Test view** — test-to-code associations, coverage evidence when available, failing tests.
7. **Runtime view** — stack traces, logs, traces, profiles and source anchors.
8. **Project-semantics view** — requirements, ADRs, architecture boundaries and owned modules.

No single view is authoritative for relevance.

## Commit awareness

Indexes MUST be tied to repository snapshots. Reusing an index from the wrong commit silently is prohibited.

Recommended identity:

```text
RepoSnapshot = hash(repo_id, base_commit, dirty_diff_hash?)
```

Incremental updates should reuse unchanged file artifacts across commits using content hashes.

## Parsing

Use deterministic parsers where possible:

- Tree-sitter for incremental multi-language syntax structure;
- language-native compiler APIs or LSP for richer symbols/types when available;
- fallback text representation for unsupported languages.

LSP data is an enhancement, not a mandatory dependency: language servers vary in quality and setup complexity.

## Code chunks

Avoid naive fixed-size token chunks as the primary representation.

Preferred units:

- declaration / function / class,
- module section,
- configuration object,
- test case,
- documentation heading section,
- runtime trace frame cluster.

Every chunk MUST retain:

- repo snapshot,
- path,
- byte/line range,
- content hash,
- structural identity if available.

## Structural graph

Minimum node types:

```text
Repository
File
Module
Symbol
Test
Requirement
Decision
RuntimeObservation
```

Minimum edge types:

```text
contains
imports
calls
references
defines
tests
covers
implements
constrained_by
changed_with
failed_at
```

Do not attempt to model every possible semantic relationship in v1.

## Change impact

When a file/symbol is edited, repository intelligence should emit likely ripple candidates based on:

- imports/references,
- tests,
- co-change history,
- interface ownership,
- runtime traces,
- requirement mapping.

This feeds both Context Engine and Verification Controller.

## Retrieval abstention

The repository service MUST be able to say “no high-confidence result.” Forcing irrelevant context is harmful.

## Update strategy

### On init

- enumerate files respecting ignore rules;
- identify languages/build systems;
- index text and supported syntax;
- optionally create embeddings asynchronously;
- discover tests/config/docs;
- build initial project map.

### On edit

- hash changed files;
- update lexical/syntax views immediately;
- update semantic embeddings lazily;
- invalidate dependent graph edges;
- refresh affected test mappings.

## Default local implementation

A sensible initial architecture:

- SQLite for metadata and graph tables;
- FTS5 or a Rust-native lexical index for text search;
- pluggable persistent HNSW/vector backend for embeddings;
- content-addressed object storage for source-derived artifacts;
- in-memory hot cache bounded by memory policy.

The vector backend MUST remain replaceable.

## Metrics

- gold/relevant file recall on eval tasks,
- context yield per 1K tokens,
- time-to-first-relevant-symbol,
- index build/update latency,
- stale-index incidents,
- repeated exploration after retrieval,
- abstention calibration.
