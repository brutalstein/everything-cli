# Intent and Requirements Engine

## Goal

Transform incomplete natural-language intent into an implementation-ready, auditable engineering contract without forcing the user to make low-value technical choices.

## Why it is a separate phase

Recent specification-level and from-scratch software-agent research shows that resolving behavioral ambiguity before synthesis materially improves implementation outcomes. Therefore intent elicitation is a first-class state machine, not just the first prompt to a coder.

## Roles

The Intent Engine performs four functions:

1. semantic extraction,
2. ambiguity/risk detection,
3. selective questioning,
4. compilation into Engineering IR.

The model used here MAY differ from the coding model. Language understanding, product reasoning, and interaction quality are primary selection dimensions.

## Question policy

AER SHOULD ask only questions whose expected value exceeds user friction.

For candidate question `q`:

```text
question_value(q) =
    expected_information_gain(q)
    × decision_impact(q)
    × irreversibility(q)
    × uncertainty(q)
    / user_friction(q)
```

A production implementation may use a calibrated learned predictor later. V1 should use explicit heuristics and log all features for future learning.

### Ask the user when

- two interpretations change externally observable behavior;
- legal/security/privacy posture depends on the answer;
- irreversible data model or compatibility choices depend on the answer;
- business scope materially changes;
- there is no responsible default.

### Decide internally when

- the choice is an implementation detail with a strong engineering default;
- it can be reversed cheaply;
- the user lacks useful information to improve the choice;
- asking would not materially reduce project risk.

Examples:

**Ask:** “Should meetings be analyzed live, after upload, or both?”  
**Usually decide:** “PostgreSQL or MySQL?”

## Intent state

The engine maintains explicit categories:

```text
UserDecisions
SystemDecisions
Assumptions
Unknowns
Constraints
Goals
NonGoals
QualityAttributes
Risks
AcceptanceCriteria
```

Unknowns receive:

- uncertainty score,
- impact score,
- evidence refs,
- recommended resolution mode: `ask_user | research | system_default | defer`.

## Interview termination

The interview ends when:

1. all high-impact unknowns are resolved or explicitly deferred;
2. acceptance criteria are testable enough for planning;
3. key constraints and non-goals are recorded;
4. remaining uncertainty falls below policy threshold.

Do not seek fictitious certainty. Unresolved uncertainties may remain in IR with explicit risk.

## Semantic checksum

For medium/high-risk projects, a second pass MUST compare:

- original user messages,
- resulting Engineering IR.

The checker asks whether any **material** requirement, prohibition, preference, or decision was omitted or distorted.

This checker does not rewrite the project freely. It emits structured deltas:

```json
{
  "missing": [],
  "distorted": [],
  "unsupported_additions": [],
  "severity": "none|low|medium|high"
}
```

High-severity mismatch blocks implementation.

## Technical decision defaults

System-decided architecture choices MUST include rationale and reversibility metadata:

```text
decision_id
choice
alternatives_considered
rationale
confidence
reversibility
trigger_for_revisit
```

This prevents hidden assumptions from becoming permanent architecture.

## Change during implementation

User intent can evolve.

A new user instruction creates a `SpecDelta`. The delta is compiled, validated, then propagated through the task graph. Tasks whose assumptions are invalidated become `stale` and MUST be reconsidered before further integration.

## Metrics

Track:

- questions per project,
- information gain proxy,
- user correction rate after IR generation,
- downstream requirement-related rework,
- semantic-checksum mismatch rate,
- percentage of system-decided vs user-decided choices,
- task failures attributable to specification error.
