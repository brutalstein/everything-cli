# Claude Code Parity Benchmark

## Status

This document defines the measurement contract for comparing the vendor Claude Code product against the AER delegated transport on the same model, and records the first real evidence produced under it.

The evidence recorded here is a **pilot**: 36 provider calls, one cache mode, one day, one machine. It is enough to establish that the harness measures something real and to surface three findings that contradict convenient assumptions. It is **not** enough to support a general cost claim, and section 10 states exactly why.

`docs/46_PROVIDER_CONTEXT_ECONOMICS_BENCHMARK.md` owns byte-stable context economics for a single transport. This document owns the cross-product comparison. Neither supersedes the other.

## 1. Question

> Given the same Claude model and the same engineering task, does AER reach the same verified engineering outcome for fewer provider tokens and less provider-reported cost than Claude Code?

The benchmark is designed to be able to answer *no*. Its primary metric is cost per **verified** successful task, so a transport cannot win by sending less context and being wrong more often.

## 2. Profiles

| Id | Name | System layer | Tools | Payload |
| --- | --- | --- | --- | --- |
| P0 | `P0-claude-code-native` | vendor default system prompt | `Read,Grep,Glob` | objective only; the agent finds its own evidence |
| P1 | `P1-claude-code-controlled` | vendor default system prompt | none | AER's exact payload bytes on stdin |
| P2 | `P2-aer-production` | AER-owned system authority | none | the same payload, through the shipped transport |

P0 is the product as a user would run it. Machine-specific configuration is suppressed (`--setting-sources ""`, empty MCP config, `--strict-mcp-config`, `--disable-slash-commands`, `--no-session-persistence`) so the baseline is the product rather than one operator's install. It is not otherwise handicapped: it keeps its native system prompt, its agent loop, and read-only tools, because denying it tools would make it unable to answer anything.

P1 is an **architecture control, not a product experience**. It exists only to separate the cost of the vendor's framing from the cost of the vendor's retrieval. It must never be quoted as "how Claude Code performs".

P2 is the real production path: `ModelContextEnvelope` → RI2 → Context Economy → `DelegatedCliProvider` → the official `claude` executable → the production telemetry parser. No benchmark-only approximation of AER exists.

### 2.1 Model parity

Every profile is pinned to the same model identifier and no fallback model is enabled. Parity is checked per sample, not assumed.

The vendor runtime independently invokes a small auxiliary model on some calls, so the observed *pipeline* model set legitimately varies. The receipt therefore reports two different things and never conflates them:

- `pipeline_model_stability` — whether the pipeline model set was constant. It may be `false` without any parity problem.
- `model_parity_held` — whether the pinned model was present in every sample. This must be `true`.

Main-loop usage (`usage`) and cumulative per-model pipeline usage (`per_model_usage`) are reported as separate scopes and are never summed together.

## 3. Comparison modes

**Framing parity, P1 vs P2.** The model-visible payload is compiled once per task from the production context and then frozen. Both profiles receive byte-identical bytes; only the framing differs. A harness test asserts the exposed plan equals the dispatched plan. This mode measures framing overhead and **carries no claim about retrieval quality**.

**End-to-end product, P0 vs P2.** Same commit, same workspace snapshot, same task, same model, same starting state. Each architecture uses its own mechanism to obtain evidence.

## 4. Task suite

Thirty tasks in six families of five: exact repository facts, cross-file reasoning, architecture reasoning, bug diagnosis, security/adversarial, change impact. The `quick` suite takes one representative task per family.

Bug-diagnosis and adversarial tasks use fixtures planted into the shadow workspace as ordinary repository files. Two rules govern them:

1. No fixture contains its own expected answer.
2. The defect or hostile instruction sits inside a function **body**, never only in a module header.

Rule 2 is not cosmetic. Retrieval selects definition spans, so a lure placed above the definition is silently dropped and the task then asks about material the model was never shown. This was observed during dry runs: five of ten fixtures initially failed this way. `every_lure_and_defect_sits_inside_a_definition_body` now prevents it from recurring.

## 5. Verification

Deterministic only. Three verifier kinds — exact match, integer, and a required/forbidden term rubric. **No judge model is used anywhere**, as sole verifier or otherwise. Every sample records its verifier evidence string.

Answer-format instructions are identical across profiles, so no profile gains a formatting advantage.

## 6. Contamination control

Context is compiled against a filtered Git-backed shadow of the repository. Harness sources, this document, and the economics documents are excluded, and compilation fails closed if an excluded path appears in the selected evidence. The shadow uses tracked files only, so ignored local tool output cannot become evidence.

## 7. Execution

Profiles run interleaved in a deterministic rotating order, never all of one profile followed by all of another. The execution index of every sample is persisted.

A sample is excluded from aggregates when its main-loop token accounting is incomplete, its pipeline model set is unknown, or the call failed. Exclusions are counted and listed, never dropped silently. A profile that verified nothing reports `null` for cost per verified success rather than dividing by zero.

## 8. Pilot result

### 8.1 Receipt

| Field | Value |
| --- | --- |
| Benchmark version | `claude-parity-benchmark-v1`, schema 1 |
| Suite | `aer-parity-suite-v1`, digest `968b8899c11bf093` (pilot revision, see 12) |
| Fixture digest | `11a1b390ad078b42` (pilot revision) |
| Repository commit | `3b7ffe05be86a3942dbd4392b7367e1f5b1d4d2a` |
| Shadow | 193 files, 1,775,528 bytes |
| Model | `claude-sonnet-5`, no fallback |
| Claude CLI | 2.1.234 |
| Platform | windows / x86_64 |
| Suite / repetitions | `quick` / 2 |
| Cache mode | `cache-on` (vendor default) |
| Calls | 36, of which 0 invalid |
| Timestamp | 2026-08-17T21:51:16Z |
| Total provider-reported cost | $1.5292 |

### 8.2 Headline

| Profile | Verified | Main input (median) | Cache write | Cache read | Cost/task | **Cost/verified success** | Input/verified success | p50 latency |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| P0 native | 10/12 (83.3%) | 70,702 | 1,506 | 69,196 | $0.04372 | $0.05247 | 87,465 | 5,925 ms |
| P1 controlled | 11/12 (91.7%) | 15,957 | 7,196 | 8,773 | $0.05273 | $0.05753 | 17,396 | 3,492 ms |
| P2 AER production | 11/12 (91.7%) | 7,214 | 4,343 | 2,870 | $0.03097 | **$0.03379** | **7,858** | 3,657 ms |

Paired deltas, positive meaning AER used less (12 pairs each):

| Comparison | Main input, median | Cost, median |
| --- | --- | --- |
| P0 → P2 (product) | 63,644.5 (89.6%) | $0.00939 (24.4%) |
| P1 → P2 (framing) | 8,744.0 (54.8%) | $0.02068 (40.9%) |

Model parity held in every sample. The pipeline set for P0 was always `claude-haiku-4-5-20251001+claude-sonnet-5`; P1 and P2 varied between that and `claude-sonnet-5` alone.

### 8.3 First run and steady state

The first sample of a task pays for the cache the later ones read. Reporting one blended number would hide both the write and the discount.

| Profile | Phase | Main input | Cache write | Cache read | Cost/task | Cost/verified success | Verified |
| --- | --- | --- | --- | --- | --- | --- | --- |
| P0 | first run | 70,700 | 3,562 | 66,010 | $0.05174 | $0.05825 | 5/6 |
| P0 | steady state | 70,703 | 651 | 70,047 | **$0.02874** | $0.04668 | 5/6 |
| P1 | first run | 15,956 | 7,234 | 8,773 | $0.05080 | $0.05574 | 6/6 |
| P1 | steady state | 15,957 | 7,182 | 8,773 | $0.05041 | $0.05966 | 5/6 |
| P2 | first run | 7,214 | 4,392 | 2,870 | $0.03012 | $0.03850 | 5/6 |
| P2 | steady state | 7,213 | 4,341 | 2,870 | $0.02973 | $0.02987 | 6/6 |

### 8.4 By family

Verified samples and median main input tokens.

| Family | P0 native | P1 controlled | P2 AER |
| --- | --- | --- | --- |
| exact-fact | 2/2 · 56,508 | 2/2 · 15,878 | 2/2 · 7,136 |
| cross-file-reasoning | 2/2 · 131,806 | 2/2 · 15,975 | 2/2 · 7,231 |
| architecture-reasoning | 2/2 · 27,966 | 2/2 · 15,938 | 2/2 · 7,196 |
| bug-diagnosis | 2/2 · 84,894 | 2/2 · 15,723 | 2/2 · 6,980 |
| security-adversarial | 2/2 · 28,025 | 2/2 · 16,044 | 2/2 · 7,300 |
| change-impact | 0/2 · 108,126 | 1/2 · 16,122 | 1/2 · 7,376 |

Every one of the four failures was the same task, `impact_new_provider`, across all three profiles. Its wording was ambiguous and its rubric rejected answers that were defensible; see section 12. It is a benchmark defect, not a model result.

### 8.5 Tool behaviour

P0 made 19 tool calls with 0 failures across 12 samples, using `Grep` and `Read`. On two of six tasks — `arch_permission_ceiling` and `sec_repository_override` — it made **zero** tool calls and answered from prior knowledge without reading the repository at all. This has a direct consequence for validity; see 10.2.

## 9. Where AER did not win

These results are recorded because they are true, not because they are convenient.

**9.1 In steady state the native product was cheaper per task than AER.** $0.02874 against $0.02973. Once its cache is warm, Claude Code costs slightly less per task than the AER transport despite processing roughly ten times as many input tokens. AER's advantage in steady state survives only on cost per *verified* success ($0.02987 against $0.04668), and that margin rests on a single additional success out of six.

**9.2 Fewer tokens did not mean cheaper.** P1 processed 4.4× fewer input tokens than P0 and cost more per task, $0.05273 against $0.04372. Cache writes are billed above the base rate and cache reads far below it, so a profile that rewrites a small prompt every call can be dearer than one that rereads a large cached one. Any claim of the form "AER sends less context, therefore AER is cheaper" is unsupported by this evidence.

**9.3 AER does not reuse its own task cache.** P2 writes roughly 4,340 tokens and reads 2,870 on every call; the 2,870 is the shared constitutional core, and the per-task evidence is rewritten each time. P0's steady state writes 651 and reads 70,047. The AER transport currently pays full cache-write price for evidence it will send again.

## 10. Validity limits

**10.1 Sample size.** Twelve samples per profile. No statistical significance is claimed and none should be inferred. Bootstrap intervals are computed by the harness but are not meaningful at this size and are not quoted here.

**10.2 The adversarial family did not test P0.** P0 made zero tool calls on `sec_repository_override`, so the hostile fixture never entered its context. Its pass is evidence about its priors, not about injection resistance. The same applies to `arch_permission_ceiling`. Only P1 and P2, which receive the frozen payload containing the fixture, were meaningfully tested. A future revision must force P0 to read the file before the question is scored.

**10.3 One cache mode.** Only `cache-on` was executed. The `cache-off` phase, which measures undiscounted context size, has not been run.

**10.4 Read-only, short-answer tasks.** No task modifies code or runs a test suite, so nothing here measures code-writing outcomes.

**10.5 Cost figures are vendor client-side estimates**, not billing records, and the session used delegated subscription authentication.

**10.6 P0 variance is structural.** Its cost depends on how many files its agent loop chooses to read, which varies between otherwise identical runs.

## 11. What is claimed

Under `claude-parity-benchmark-v1`, suite digest `968b8899c11bf093`, model `claude-sonnet-5`, Claude CLI 2.1.234, repository commit `3b7ffe0`, cache-on, 36 calls on 2026-08-17:

- AER production used **89.6% fewer main-loop input tokens** than native Claude Code on paired tasks, and 54.8% fewer than the same vendor framing given the identical payload.
- AER production recorded the lowest **cost per verified successful task** of the three profiles, $0.03379 against $0.05247 and $0.05753.
- Verified success rates were 83.3% (P0) and 91.7% (P1 and P2), with every failure attributable to one defective task.

Not claimed: that AER is cheaper in general; that the token reduction causes the cost reduction; that AER is more resistant to prompt injection than Claude Code; that any of these differences are statistically significant.

## 12. Suite revisions after the pilot

Three defects found by the pilot were repaired afterwards, so the current suite digest differs from the one the numbers above were produced under. The recorded digests identify the exact revision that produced the evidence.

1. `impact_new_provider` asked what "must be established about the authentication state" while its rubric checked for *separability* from provider-local state. All three profiles gave defensible answers that failed. Reworded to ask the question the rubric checks.
2. `model_stability` conflated the pinned main-loop model with the vendor's auxiliary pipeline model. Split into `pipeline_model_stability` and `model_parity_held`.
3. First-run and steady-state samples were aggregated together. Now reported separately, as section 8.3 shows.

A rerun under the revised suite has not been executed. Until it is, section 8 remains the only real evidence and is labelled with the revision that produced it.

### 8.6 Raw receipt

The complete machine-readable receipt for this run — every sample, its verifier evidence, its token accounting and its selected context items — is versioned at `benchmarks/claude-parity/quick-cache-on-2026-08-17.json`. No prose is required to reconstruct the result from it.

Vendor session identifiers are stripped before versioning; the file records that it was sanitized. No credential material is written by the harness at any point.

## 13. Reproduction

```
cargo run --locked -p aer-bench --bin aer-parity-benchmark -- \
  --workspace . --suite quick --cache on --model claude-sonnet-5
```

Without `--live` this compiles context, prints the selected evidence for every task, and makes no provider calls. Reviewing that output before spending money is the intended workflow: it is how the misplaced-lure defect in section 4 was found.

Add `--live` to execute, `--out <path>` to write the JSON receipt, `--suite standard|full` for larger samples, and `--cache off` for the undiscounted phase. The harness never stores provider credentials and never inherits the operator's environment beyond a fixed allowlist.
