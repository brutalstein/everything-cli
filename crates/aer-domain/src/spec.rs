//! Deterministic intent, decision, Engineering-IR, and semantic-checksum semantics.
//!
//! This module deliberately has no provider, storage, JSON, or UI dependency.
//! Adapters may extract or persist these values, but authority and validation
//! semantics remain deterministic and model-independent.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceKind {
    UserMessage,
    ResearchClaim,
    SystemDefault,
    Repository,
    ArchitectureDecision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRef {
    pub kind: SourceKind,
    pub id: String,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticStatus {
    Proposed,
    Accepted,
    Deprecated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticItem {
    pub id: String,
    pub statement: String,
    pub source_refs: Vec<SourceRef>,
    pub status: SemanticStatus,
    pub risk: Risk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequirementPriority {
    Must,
    Should,
    Could,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Requirement {
    pub item: SemanticItem,
    pub priority: RequirementPriority,
    pub verification_strategy: Vec<String>,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptanceCriterion {
    pub item: SemanticItem,
    pub requirement_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionAuthority {
    User,
    System,
    Organization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reversibility {
    Easy,
    Moderate,
    Hard,
    Irreversible,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decision {
    pub id: String,
    pub choice: String,
    pub authority: DecisionAuthority,
    pub rationale: String,
    /// Integer confidence in thousandths, inclusive range 0..=1000.
    pub confidence_milli: u16,
    pub reversibility: Reversibility,
    pub source_refs: Vec<SourceRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnknownResolution {
    AskUser,
    Research,
    SystemDefault,
    Defer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unknown {
    pub id: String,
    pub question: String,
    /// Integer score in thousandths, inclusive range 0..=1000.
    pub uncertainty_milli: u16,
    /// Integer score in thousandths, inclusive range 0..=1000.
    pub impact_milli: u16,
    /// Integer score in thousandths, inclusive range 0..=1000.
    pub irreversibility_milli: u16,
    /// Integer score in thousandths. Zero is treated as minimum non-zero friction.
    pub friction_milli: u16,
    pub resolution: UnknownResolution,
    pub evidence_refs: Vec<SourceRef>,
}

impl Unknown {
    /// Deterministic V1 approximation of the architecture's question-value policy.
    #[must_use]
    pub fn question_value(&self) -> u64 {
        let numerator = u128::from(self.uncertainty_milli)
            * u128::from(self.impact_milli)
            * u128::from(self.irreversibility_milli);
        let friction = u128::from(self.friction_milli.max(1));
        u64::try_from(numerator / friction).unwrap_or(u64::MAX)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserMessage {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResearchFinding {
    pub research_id: String,
    pub claim_id: String,
    pub statement: String,
    pub status: ResearchClaimStatus,
    pub confidence_milli: u16,
    pub source_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResearchClaimStatus {
    Supported,
    Contested,
    Insufficient,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentState {
    pub messages: Vec<UserMessage>,
    pub user_decisions: Vec<Decision>,
    pub system_decisions: Vec<Decision>,
    pub assumptions: Vec<SemanticItem>,
    pub unknowns: Vec<Unknown>,
    pub constraints: Vec<SemanticItem>,
    pub goals: Vec<SemanticItem>,
    pub non_goals: Vec<SemanticItem>,
    pub quality_attributes: Vec<SemanticItem>,
    pub risks: Vec<SemanticItem>,
    pub functional_requirements: Vec<Requirement>,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
}

impl IntentState {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            messages: Vec::new(),
            user_decisions: Vec::new(),
            system_decisions: Vec::new(),
            assumptions: Vec::new(),
            unknowns: Vec::new(),
            constraints: Vec::new(),
            goals: Vec::new(),
            non_goals: Vec::new(),
            quality_attributes: Vec::new(),
            risks: Vec::new(),
            functional_requirements: Vec::new(),
            acceptance_criteria: Vec::new(),
        }
    }

    /// Returns the highest-value user question with stable ID tie-breaking.
    #[must_use]
    pub fn next_user_question(&self) -> Option<&Unknown> {
        self.unknowns
            .iter()
            .filter(|unknown| unknown.resolution == UnknownResolution::AskUser)
            .max_by(|left, right| {
                left.question_value()
                    .cmp(&right.question_value())
                    .then_with(|| right.id.cmp(&left.id))
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectDescriptor {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineeringIr {
    pub schema_version: u32,
    pub project: ProjectDescriptor,
    pub goals: Vec<SemanticItem>,
    pub non_goals: Vec<SemanticItem>,
    pub functional_requirements: Vec<Requirement>,
    pub quality_attributes: Vec<SemanticItem>,
    pub constraints: Vec<SemanticItem>,
    pub invariants: Vec<SemanticItem>,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub risks: Vec<SemanticItem>,
    pub decisions: Vec<Decision>,
    pub unknowns: Vec<Unknown>,
    pub assumptions: Vec<SemanticItem>,
    pub research_findings: Vec<ResearchFinding>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecDelta {
    pub base_revision: u64,
    pub new_revision: u64,
    pub source_ref: SourceRef,
    pub added_ids: Vec<String>,
    pub changed_ids: Vec<String>,
    pub invalidated_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ChecksumSeverity {
    None,
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticChecksum {
    pub missing: Vec<String>,
    pub distorted: Vec<String>,
    pub unsupported_additions: Vec<String>,
    pub severity: ChecksumSeverity,
}

/// Deterministically compares user-authoritative semantic state with compiled IR.
/// Accepted material may not disappear, change meaning under the same stable ID,
/// or appear without provenance.
#[must_use]
pub fn semantic_checksum(intent: &IntentState, ir: &EngineeringIr) -> SemanticChecksum {
    let mut missing = Vec::new();
    let mut distorted = Vec::new();
    let mut unsupported_additions = Vec::new();

    compare_items(&intent.goals, &ir.goals, &mut missing, &mut distorted);
    compare_items(
        &intent.non_goals,
        &ir.non_goals,
        &mut missing,
        &mut distorted,
    );
    compare_items(
        &intent.constraints,
        &ir.constraints,
        &mut missing,
        &mut distorted,
    );
    compare_items(
        &intent.quality_attributes,
        &ir.quality_attributes,
        &mut missing,
        &mut distorted,
    );
    compare_items(
        &intent.assumptions,
        &ir.assumptions,
        &mut missing,
        &mut distorted,
    );

    for expected in &intent.acceptance_criteria {
        compare_one(
            &expected.item,
            ir.acceptance_criteria
                .iter()
                .map(|criterion| &criterion.item),
            &mut missing,
            &mut distorted,
        );
    }
    for expected in &intent.functional_requirements {
        compare_one(
            &expected.item,
            ir.functional_requirements
                .iter()
                .map(|requirement| &requirement.item),
            &mut missing,
            &mut distorted,
        );
    }

    for item in ir
        .goals
        .iter()
        .chain(ir.non_goals.iter())
        .chain(ir.constraints.iter())
        .chain(ir.quality_attributes.iter())
        .chain(ir.invariants.iter())
        .chain(ir.assumptions.iter())
        .chain(
            ir.acceptance_criteria
                .iter()
                .map(|criterion| &criterion.item),
        )
        .chain(
            ir.functional_requirements
                .iter()
                .map(|requirement| &requirement.item),
        )
    {
        if item.status == SemanticStatus::Accepted && item.source_refs.is_empty() {
            unsupported_additions.push(item.id.clone());
        }
    }

    missing.sort();
    missing.dedup();
    distorted.sort();
    distorted.dedup();
    unsupported_additions.sort();
    unsupported_additions.dedup();

    let severity =
        if !missing.is_empty() || !distorted.is_empty() || !unsupported_additions.is_empty() {
            ChecksumSeverity::High
        } else {
            ChecksumSeverity::None
        };

    SemanticChecksum {
        missing,
        distorted,
        unsupported_additions,
        severity,
    }
}

fn compare_items(
    expected: &[SemanticItem],
    actual: &[SemanticItem],
    missing: &mut Vec<String>,
    distorted: &mut Vec<String>,
) {
    for item in expected
        .iter()
        .filter(|item| item.status == SemanticStatus::Accepted)
    {
        compare_one(item, actual.iter(), missing, distorted);
    }
}

fn compare_one<'a>(
    expected: &SemanticItem,
    actual: impl Iterator<Item = &'a SemanticItem>,
    missing: &mut Vec<String>,
    distorted: &mut Vec<String>,
) {
    match actual.into_iter().find(|item| item.id == expected.id) {
        None => missing.push(expected.id.clone()),
        Some(item) if item.statement != expected.statement => distorted.push(expected.id.clone()),
        Some(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChecksumSeverity, EngineeringIr, IntentState, ProjectDescriptor, Risk, SemanticItem,
        SemanticStatus, SourceKind, SourceRef, Unknown, UnknownResolution, semantic_checksum,
    };

    fn source() -> SourceRef {
        SourceRef {
            kind: SourceKind::UserMessage,
            id: "MSG-1".to_owned(),
            detail: None,
        }
    }

    fn item(id: &str, statement: &str) -> SemanticItem {
        SemanticItem {
            id: id.to_owned(),
            statement: statement.to_owned(),
            source_refs: vec![source()],
            status: SemanticStatus::Accepted,
            risk: Risk::Medium,
        }
    }

    fn ir(goal: SemanticItem) -> EngineeringIr {
        EngineeringIr {
            schema_version: 1,
            project: ProjectDescriptor {
                id: "project".to_owned(),
                title: "project".to_owned(),
                summary: "summary".to_owned(),
            },
            goals: vec![goal],
            non_goals: Vec::new(),
            functional_requirements: Vec::new(),
            quality_attributes: Vec::new(),
            constraints: Vec::new(),
            invariants: Vec::new(),
            acceptance_criteria: Vec::new(),
            risks: Vec::new(),
            decisions: Vec::new(),
            unknowns: Vec::new(),
            assumptions: Vec::new(),
            research_findings: Vec::new(),
        }
    }

    #[test]
    fn question_policy_is_deterministic_and_value_ordered() {
        let mut state = IntentState::empty();
        state.unknowns.push(Unknown {
            id: "UNK-B".to_owned(),
            question: "low".to_owned(),
            uncertainty_milli: 500,
            impact_milli: 500,
            irreversibility_milli: 500,
            friction_milli: 500,
            resolution: UnknownResolution::AskUser,
            evidence_refs: Vec::new(),
        });
        state.unknowns.push(Unknown {
            id: "UNK-A".to_owned(),
            question: "high".to_owned(),
            uncertainty_milli: 900,
            impact_milli: 900,
            irreversibility_milli: 900,
            friction_milli: 200,
            resolution: UnknownResolution::AskUser,
            evidence_refs: Vec::new(),
        });
        assert_eq!(state.next_user_question().expect("question").id, "UNK-A");
    }

    #[test]
    fn semantic_checksum_blocks_distortion_and_unsupported_addition() {
        let expected = item("GOAL-1", "preserve this exact user goal");
        let mut intent = IntentState::empty();
        intent.goals.push(expected.clone());

        let valid = ir(expected.clone());
        assert_eq!(
            semantic_checksum(&intent, &valid).severity,
            ChecksumSeverity::None
        );

        let mut distorted = valid.clone();
        distorted.goals[0].statement = "changed meaning".to_owned();
        let result = semantic_checksum(&intent, &distorted);
        assert_eq!(result.severity, ChecksumSeverity::High);
        assert_eq!(result.distorted, vec!["GOAL-1"]);

        let mut unsupported = valid;
        unsupported.goals.push(SemanticItem {
            id: "GOAL-FAKE".to_owned(),
            statement: "model invented".to_owned(),
            source_refs: Vec::new(),
            status: SemanticStatus::Accepted,
            risk: Risk::Low,
        });
        let result = semantic_checksum(&intent, &unsupported);
        assert_eq!(result.severity, ChecksumSeverity::High);
        assert_eq!(result.unsupported_additions, vec!["GOAL-FAKE"]);
    }
}
