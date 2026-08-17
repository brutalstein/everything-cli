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

## 7. Target-Windows Claude baselines

### 7.1 Pre-promotion baseline — vendor coding-agent preset

The repeated delegated-Claude benchmark established a stable main-loop plateau around:

- ~11.2k exact provider input tokens;
- ~4.2k cache-read tokens;
- ~6.9–7.1k cache-creation tokens after the first call;
- near-zero fresh input.

The model-visible AER context was stable, so the remaining cache-write plateau needed attribution rather than blind ContextSizer changes.

### 7.2 Production baseline — AER authority split

Recorded after the authority split became the production delegated Claude transport, with the same canonical probe (`provider-context-economics-v1`, 3 runs, target Windows):

| Dimension | Observed |
| --- | --- |
| exact main-loop input (median / min / max) | 7144 / 7144 / 7144 tokens |
| exact-input spread | 0 tokens |
| fresh input (median) | 2 tokens (2 bps) |
| cache creation (median) | 4272 tokens (5979 bps) |
| cache read (median) | 2870 tokens (4017 bps) |
| first-call to steady-state cache-write delta | 0 tokens |
| first-call to steady-state cache-read delta | 0 tokens |
| output | 17 tokens, contract pass |
| provider-reported cost | $0.029039 per call |
| latency | 3149–3202 ms |
| resolved models | `claude-haiku-4-5-20251001`, `claude-sonnet-5` (stable) |
| model-visible context digest | `f410f393…` (stable across all runs) |
| measurement validity | `valid: true` |

Against §7.1 that is roughly a 4.0k-token reduction in exact main-loop input and roughly a 40% reduction in provider-reported cost per comparable call. Cache-read tokens also fell (4.2k → 2.87k) because the whole request is smaller; per §11 a lower absolute cache-read count is not a regression when more total input was removed than was lost from reads.

The `modelUsage` breakdown shows the reduction lands where expected: the main-loop model (`claude-sonnet-5`) sees fresh input of 2 tokens with the AER authority prefix served from cache read, and the auxiliary `claude-haiku-4-5-20251001` record accounts for the remaining uncached input.

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

In the post-repair, post-promotion target-Windows 6-task × 2-profile × 2-run matrix (`claude-authority-split-acceptance-v3`), the production authority-split transport measured against the retained non-production `legacy-claude-preset` comparator:

- median paired main-input reduction across tasks: **4280 provider tokens**;
- median paired provider-cost reduction: **$0.01775 per call**;
- median paired cache-creation reduction: **2931 tokens**;
- median paired cache-read change: **−1349 tokens** (production reads fewer because the whole request is smaller);
- median paired latency reduction: **223 ms** (small sample; not a reliable directional claim);
- total provider-reported cost across the matrix: legacy **$0.5776**, production **$0.3633**.

`production_measurements_valid` was `true` for all six tasks. `legacy_measurements_valid` was `false`: on three tasks the vendor preset resolved `['claude-sonnet-5']` on one run and `['claude-haiku-4-5-20251001', 'claude-sonnet-5']` on the other, which fails the stable-resolved-model requirement of §5. That instability belongs to the retired comparator, not to the production transport, and it widens the uncertainty band on the paired economics above without affecting the acceptance decision.

## 10. Economics did not promote the candidate

The exact-definition retrieval defect that previously blocked this section is repaired: both profiles now answer `3` for the value assigned by `ArchitectureContextCapsule::compile`, and the defining span is present in the selected Context Pack.

Promotion was decided by the acceptance gate in `47_PROVIDER_AUTHORITY_SPLIT_ACCEPTANCE.md` — authority safety, adversarial defense, source-grounded correctness and measurement validity — not by the numbers in §7.2 and §9. Those numbers are recorded consequences of the promotion, not its justification.

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

Steps 1–6 are complete: exact-identifier defining-span retrieval was repaired with deterministic regressions, the repository gates and canonical Windows verification were rerun, the live acceptance matrix passed, the authority split was promoted to the production delegated Claude transport, and `provider-context-economics-v1` was rerun against it (§7.2).

Remaining:

1. calibrate ContextSizer/retrieval budgets from the §7.2 production baseline rather than from the retired §7.1 plateau;
2. establish the strong sandbox boundary required by `45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md` before any tool-capable delegated path is considered.

Do not tune the old ~7k cache-write plateau as if it were current: it belongs to the retired vendor-preset transport.

## 13. Related documents

- `45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md` — normative provider authority/runtime contract.
- `47_PROVIDER_AUTHORITY_SPLIT_ACCEPTANCE.md` — production-candidate correctness and adversarial acceptance.
- root `STATUS.md` — current implementation/gate truth.
