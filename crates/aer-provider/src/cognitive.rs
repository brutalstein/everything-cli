#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CognitiveWorkRole {
    Deterministic,
    Scout,
    Locator,
    RepositoryExplorer,
    FailureAnalyst,
    EvidenceCompressor,
    Planner,
    Coder,
    SemanticReviewer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CognitiveWorkPolicy {
    pub requires_model: bool,
    pub permits_low_cost_scout_tier: bool,
    pub requires_structured_output: bool,
    pub requires_tool_calling: bool,
    pub requires_frontier_reasoning: bool,
}

impl CognitiveWorkRole {
    #[must_use]
    pub const fn policy(self) -> CognitiveWorkPolicy {
        match self {
            Self::Deterministic => CognitiveWorkPolicy {
                requires_model: false,
                permits_low_cost_scout_tier: false,
                requires_structured_output: false,
                requires_tool_calling: false,
                requires_frontier_reasoning: false,
            },
            Self::Scout | Self::Locator | Self::RepositoryExplorer => CognitiveWorkPolicy {
                requires_model: true,
                permits_low_cost_scout_tier: true,
                requires_structured_output: true,
                requires_tool_calling: false,
                requires_frontier_reasoning: false,
            },
            Self::FailureAnalyst | Self::EvidenceCompressor => CognitiveWorkPolicy {
                requires_model: true,
                permits_low_cost_scout_tier: true,
                requires_structured_output: true,
                requires_tool_calling: false,
                requires_frontier_reasoning: false,
            },
            Self::Planner => CognitiveWorkPolicy {
                requires_model: true,
                permits_low_cost_scout_tier: false,
                requires_structured_output: true,
                requires_tool_calling: false,
                requires_frontier_reasoning: true,
            },
            Self::Coder | Self::SemanticReviewer => CognitiveWorkPolicy {
                requires_model: true,
                permits_low_cost_scout_tier: false,
                requires_structured_output: true,
                // Tool execution remains disabled by the current runtime. This
                // role describes cognitive capability, not permission authority.
                requires_tool_calling: false,
                requires_frontier_reasoning: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_work_never_requires_a_provider() {
        let policy = CognitiveWorkRole::Deterministic.policy();
        assert!(!policy.requires_model);
        assert!(!policy.permits_low_cost_scout_tier);
    }

    #[test]
    fn scouting_can_use_a_cheaper_model_but_coding_cannot_be_silently_downgraded() {
        assert!(CognitiveWorkRole::Scout.policy().permits_low_cost_scout_tier);
        assert!(!CognitiveWorkRole::Coder.policy().permits_low_cost_scout_tier);
        assert!(CognitiveWorkRole::Coder.policy().requires_frontier_reasoning);
    }
}
