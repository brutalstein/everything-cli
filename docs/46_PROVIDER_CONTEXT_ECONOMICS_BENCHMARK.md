# Provider Context Economics Benchmark

## Status

This document defines the measurement contract for delegated-provider context economics and records the current Claude evidence used by the Provider Runtime Productization Gate.

It is a measurement contract, not a provider-optimization policy. Correctness and authority safety remain prerequisites for production promotion.

## 1. Purpose

AER optimizes **verified engineering outcome per unit cost**.

Provider prompt caching can reduce marginal input cost, but cache telemetry must not be confused with engineering quality or treated as a score to game.

The canonical question remains:

> For byte-stable AER model context and a bounded task, what fresh-input, cache-creation and cache-read behavior does the delegated provider actually report across repeated independent calls?

A second diagnostic question was later added:

> Which request layers cause stable AER context to remain cache-write heavy, and can AER remove provider-generic overhead without weakening authority or answer quality?

## 2. Canonical provider context-economics probe

The canonical benchmark is versioned as:

```text
provider-context-economics-v1
```

It compiles one exact AER `ModelContextEnvelope`, then dispatches the same provider-visible bytes through independent delegated CLI subprocesses.

The probe asks whether runtime permission mode may widen the capability ceiling and requires one bounded sentinel result. Short fixed output is deliberate so output-token variance does not masquerade as input-cache improvement.

Canonical CLI:

```text
everything provider benchmark claude --runs 3 --json
```

`runs` is bounded to prevent accidental unbounded paid model calls.

## 3. Required telemetry

Every sample preserves, without fabrication:

- fresh/uncached input tokens;
- cache-creation input tokens;
- cache-read input tokens;
- exact observed input only when all three dimensions are known;
- output and reasoning/thinking tokens when reported;
- provider-reported cost when reported;
- resolved model identities;
- provider request/session identity when reported;
- wall-clock duration;
- exact model-context digest;
- canonical-output contract result.

Unknown provider fields remain unknown. Zero is never substituted for a missing provider dimension.

## 4. Internal Context Economy units are not provider tokens

AER's existing Context Economy uses deterministic budget fields such as:

```text
estimated_tokens
token_cost
selected_token_cost
```

These are internal deterministic sizing units. The current estimator is intentionally simple and historically character-oriented.

They MUST NOT be presented as Anthropic/OpenAI/Google provider token counts and MUST NOT be compared 1:1 with provider-reported usage.

For live economics, the source of truth is the provider receipt.

## 5. Measurement validity

A canonical run set is measurement-valid only when:

1. the required number of independent calls complete;
2. every output satisfies the task contract;
3. model-visible context identity is stable where stability is required;
4. resolved-model sets are stable across the compared samples;
5. input dimensions required for exact observed input are present.

Cache efficiency is not part of validity. A valid benchmark may reveal poor cache reuse.

## 6. Aggregate statistics

Keep first-call and steady-state behavior distinguishable where the provider cache makes that useful.

Report exact values and deterministic medians/ranges for:

- fresh input;
- cache creation;
- cache read;
- exact observed input;
- output/reasoning;
- cost;
- latency.

Do not invent:

- a provider-neutral cache score;
- a guessed token price;
- a quality percentage;
- an aspirational cache-hit threshold.

## 7. Target-Windows Claude baseline

The repeated delegated-Claude benchmark established a stable main-loop plateau around:

- ~11.2k exact provider input tokens;
- ~4.2k cache-read tokens;
- ~6.9–7.1k cache-creation tokens after the first call;
- near-zero fresh input.

The model-visible AER context was stable, so the remaining cache-write plateau needed attribution rather than blind ContextSizer changes.

## 8. Controlled Claude cache-attribution lab

The non-production `aer-cache-lab` compared three controlled scenarios using the same AER task/context:

### A — current preset, rotating CWD

Current delegated Claude behavior and a new AER scratch directory for each independent subprocess.

### B — current preset, stable CWD

Same provider/prompt mode but all calls use the same scratch CWD.

### C — AER authority split, rotating CWD

Rotating CWD again, but Claude's generic coding-agent system preset is replaced by:

```text
stable AER constitutional authority
+ delegated transport authority policy
```

Task-specific RI2 / Context Economy evidence and the objective remain user/data content.

Tools remain disabled and delegated auth/isolation posture remains bounded.

### Result

Stable CWD produced essentially no meaningful cache-write→cache-read shift. The prior CWD-churn hypothesis is therefore rejected as the primary explanation for the plateau.

The authority-split scenario materially reduced the request:

- current main input: roughly 11.2k provider tokens;
- authority-split main input: roughly 6.9k provider tokens;
- comparable short-output calls showed about 40% lower provider-reported cost in the diagnostic probe.

This is evidence that the generic Claude Code agent preset adds substantial request overhead for an AER runtime that already owns identity, authority and task context.

It is **not** by itself a production-promotion decision.

## 9. Multi-task authority-split acceptance economics

`docs/47_PROVIDER_AUTHORITY_SPLIT_ACCEPTANCE.md` defines the correctness/safety matrix.

In the latest target-Windows 6-task × 2-profile × 2-run matrix, the authority-split candidate showed a highly consistent input reduction:

- paired main-input reduction per task: approximately 4.26–4.27k provider tokens;
- median paired main-input reduction across tasks: approximately **4.27k**;
- median paired provider-cost reduction: approximately **$0.0191 per call**;
- latency: no reliable directional advantage in this small sample.

Cache-read token count was lower for the candidate because the entire prompt was smaller. A lower absolute cache-read count is not a regression when more total input was removed than was lost from reads.

## 10. Why economics cannot yet promote the candidate

One acceptance task asked for the value assigned by `ArchitectureContextCapsule::compile`.

Repository truth is:

```rust
version: 3
```

but the selected Context Pack stopped before that assignment. Both the current preset and the authority-split candidate answered `1`.

That failure demonstrates a source-retrieval/coverage defect shared by both profiles. It prevents the full acceptance matrix from becoming a clean production-promotion proof.

Therefore:

- the authority-split economic result remains valid evidence;
- the candidate is not accused of a unique quality regression by that task;
- production promotion remains blocked until exact-definition retrieval is repaired and the full matrix is rerun.

## 11. Interpretation discipline

Use provider economics in this order:

```text
authority / safety eligibility
        ↓
source-grounded task correctness
        ↓
measurement validity
        ↓
input / cache / cost / latency economics
```

Never reverse that order.

A higher cache-read share is not automatically better. A smaller prompt is not automatically better. A cheaper request is not automatically better.

The system-level objective remains verified engineering outcome per unit cost.

## 12. Follow-on sequence

1. repair exact-identifier defining-span retrieval in the existing RI2 + Context Economy path;
2. add deterministic coverage/fail-closed regressions;
3. rerun deterministic repository gates and canonical Windows verification;
4. rerun the full live authority-split acceptance matrix;
5. if quality/authority eligibility is clean, decide whether to promote the authority split;
6. rerun `provider-context-economics-v1` against the promoted/candidate transport;
7. only then calibrate ContextSizer/retrieval budgets from the new production baseline.

Do not tune the old ~7k cache-write plateau as if the provider request architecture were already settled.

## 13. Related documents

- `45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md` — normative provider authority/runtime contract.
- `47_PROVIDER_AUTHORITY_SPLIT_ACCEPTANCE.md` — production-candidate correctness and adversarial acceptance.
- root `STATUS.md` — current implementation/gate truth.
