# Research and External Knowledge Acquisition

## 1. Objective

AER uses research to resolve unknowns that cannot be answered responsibly from the repository, Engineering IR, or deterministic tooling.

External information is **untrusted evidence**, not authority. A retrieved page, paper, issue, tool output, or another agent may inform a decision but cannot grant capabilities or silently become a verified project fact.

## 2. Why this is a subsystem

The existing architecture already contains:

- `research` tasks,
- unknowns whose resolution mode is `research`,
- `research_findings` in Engineering IR,
- research network policy,
- prompt-injection defenses.

Without a defined pipeline these concepts have no reliable lifecycle.

Recent deep-research studies show two relevant failure modes:

- concentrated user-generated sources can poison multi-query research pipelines: https://arxiv.org/abs/2605.24245
- apparently credible misleading knowledge can survive long-horizon research and induce false conclusions: https://arxiv.org/abs/2607.20891

AER therefore requires claim-level provenance, source diversity, freshness, and explicit uncertainty.

## 3. Research pipeline

```text
ResearchQuestion
    ↓
SearchPlan
    ↓
Source Discovery
    ↓
Source Classification + Safety Labeling
    ↓
Claim Extraction
    ↓
Corroboration / Contradiction Search
    ↓
Claim Confidence + Freshness
    ↓
ResearchArtifact
    ↓
Semantic promotion decision
```

Research output MUST validate against `schemas/research-artifact.schema.json`.

## 4. Source hierarchy

Source preference is task-dependent, but the default hierarchy is:

1. current official specifications/documentation,
2. primary research papers / standards,
3. authoritative source repositories and release notes,
4. reputable secondary technical sources,
5. community/user-generated content,
6. anonymous/unknown provenance content.

Lower-tier content MAY discover hypotheses or implementation tips. It SHOULD NOT be the sole support for high-impact architecture, security, legal, compatibility, or API-fact decisions when a primary source exists.

## 5. Temporal semantics

Every source and claim carries:

- retrieval time,
- publication/update time when known,
- version/snapshot when applicable,
- freshness class,
- content hash,
- invalidation/recheck trigger.

A claim such as “model X supports feature Y” is time-sensitive and MUST NOT be stored as timeless fact.

Research tied to an external version SHOULD name that version explicitly.

## 6. Claim model

A research artifact is claim-oriented rather than report-oriented.

Each claim includes:

```text
claim_id
statement
status = supported | contested | insufficient
confidence
source_refs[]
counterevidence_refs[]
freshness
scope
decision_refs[]
```

Confidence is not a model feeling. It is a calibrated summary of evidence quality, agreement, directness, and freshness.

## 7. Corroboration policy

High-impact claims SHOULD seek independent corroboration where practical.

Independence means different underlying evidence, not three articles repeating the same press release.

Contradictions MUST be preserved. The model must not collapse disagreement into a single confident summary merely for narrative neatness.

If sources disagree materially:

- record both positions,
- identify source/version differences,
- lower confidence,
- create an unresolved question or controlled experiment where possible.

## 8. Research-to-Engineering-IR promotion

A ResearchArtifact does not directly mutate accepted requirements or verified facts.

Promotion paths:

```text
research claim
  -> proposed system decision
  -> architecture decision / ADR
```

or:

```text
research claim
  -> hypothesis
  -> local executable experiment
  -> verified fact
```

For external facts that cannot be locally verified, the Engineering State stores them as source-backed external claims with freshness metadata rather than pretending they are executable evidence.

## 9. Prompt-injection isolation

Fetched content is data.

Research workers:

- MUST NOT treat page text as runtime policy;
- MUST NOT execute commands merely because a source instructs them to;
- MUST NOT expand network/filesystem authority based on retrieved content;
- SHOULD separate source content from system/task instructions in the cognitive adapter;
- MUST label suspicious instruction-like content;
- MUST preserve the source URI/hash for later inspection.

## 10. Search and source budget

Research has explicit budgets:

```text
queries
sources_fetched
bytes
model_tokens
wall_time
cost
```

The engine SHOULD stop when marginal evidence gain becomes low, not after an arbitrary number of searches.

## 11. Research caching

Cached sources MAY be reused when:

- the content hash or immutable version is known,
- freshness policy allows reuse,
- the current question falls within source scope.

Time-sensitive sources require revalidation.

## 12. Research verification

Research quality metrics include:

- primary-source ratio,
- claim citation coverage,
- contradiction discovery rate,
- stale-source rate,
- unsupported-claim rate,
- citation-to-claim entailment,
- duplicate-source-family rate,
- prompt-injection incidents,
- downstream decision reversals caused by bad research.

## 13. Network/security integration

Research network access uses the sandbox/network broker policy from `13`.

A research task does not automatically receive unrestricted browsing. Organization allow/deny policy, data sensitivity, region constraints, and authentication still apply.

## 14. User-facing behavior

Normal users should see concise outcomes:

```text
Research resolved 3 architecture unknowns
1 remains contested
```

Inspection MAY show claim/source graphs. Raw browsing transcripts do not belong in the default UI.
