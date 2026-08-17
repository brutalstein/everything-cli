use std::{collections::BTreeSet, error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContextTrustClass {
    SystemAuthority,
    UntrustedData,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContextSemanticRole {
    Constitution,
    ProjectOrientation,
    TaskEvidence,
    DecisionCriticalEvidence,
    IterationDelta,
    UserObjective,
    OutputContract,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContextReuseScope {
    Global,
    Project,
    Snapshot,
    Task,
    Iteration,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContextVolatility {
    Immutable,
    ProjectStable,
    SnapshotStable,
    TaskStable,
    IterationDynamic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSegment {
    pub id: String,
    pub semantic_role: ContextSemanticRole,
    pub trust_class: ContextTrustClass,
    pub reuse_scope: ContextReuseScope,
    pub volatility: ContextVolatility,
    pub content_hash: String,
    pub token_estimate: u32,
    pub source_refs: Vec<String>,
    pub rendered_bytes: String,
}

impl ContextSegment {
    #[must_use]
    pub fn provider_visible_len(&self) -> usize {
        self.rendered_bytes.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCacheFamily {
    None,
    ImplicitCommonPrefix,
    ExplicitPrefixBreakpoints,
    CachedContextObject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCacheGranularity {
    Prompt,
    Message,
    Block,
    ContextObject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCacheTtl {
    Session,
    FiveMinutes,
    OneHour,
    ProviderManaged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCacheCapabilities {
    pub family: ProviderCacheFamily,
    pub minimum_cacheable_prefix_bytes: Option<usize>,
    pub maximum_breakpoints: usize,
    pub supported_ttls: Vec<ProviderCacheTtl>,
    pub cached_read_telemetry: bool,
    pub cache_write_telemetry: bool,
    pub stable_prefix_required: bool,
    pub granularity: ProviderCacheGranularity,
    pub cache_key_supported: bool,
}

impl ProviderCacheCapabilities {
    #[must_use]
    pub const fn no_cache() -> Self {
        Self {
            family: ProviderCacheFamily::None,
            minimum_cacheable_prefix_bytes: None,
            maximum_breakpoints: 0,
            supported_ttls: Vec::new(),
            cached_read_telemetry: false,
            cache_write_telemetry: false,
            stable_prefix_required: false,
            granularity: ProviderCacheGranularity::Prompt,
            cache_key_supported: false,
        }
    }

    /// Capability description for the current delegated Claude CLI transport.
    ///
    /// AER has observed provider telemetry for cache creation/read usage and a
    /// reusable stable prompt prefix. The CLI transport does not expose legal
    /// independent per-file cache objects or AER-controlled breakpoints, so this
    /// is deliberately only an implicit common-prefix capability.
    #[must_use]
    pub fn delegated_claude_cli() -> Self {
        Self {
            family: ProviderCacheFamily::ImplicitCommonPrefix,
            minimum_cacheable_prefix_bytes: None,
            maximum_breakpoints: 0,
            supported_ttls: vec![ProviderCacheTtl::ProviderManaged],
            cached_read_telemetry: true,
            cache_write_telemetry: true,
            stable_prefix_required: true,
            granularity: ProviderCacheGranularity::Prompt,
            cache_key_supported: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextAssemblyPlan {
    pub ordered_segments: Vec<ContextSegment>,
    /// Segment-boundary indexes after which an explicit-cache transport may
    /// legally place a breakpoint. Empty for transports without such a feature.
    pub cache_breakpoints: Vec<usize>,
    pub stable_bytes: usize,
    pub dynamic_bytes: usize,
    pub provider_visible_bytes: usize,
}

impl ContextAssemblyPlan {
    #[must_use]
    pub fn semantic_identities(&self) -> BTreeSet<(&str, &str)> {
        self.ordered_segments
            .iter()
            .map(|segment| (segment.id.as_str(), segment.content_hash.as_str()))
            .collect()
    }

    #[must_use]
    pub fn render(&self) -> String {
        let mut rendered = String::new();
        for segment in &self.ordered_segments {
            if !rendered.is_empty() && !rendered.ends_with('\n') {
                rendered.push('\n');
            }
            rendered.push_str(&segment.rendered_bytes);
            if !rendered.ends_with('\n') {
                rendered.push('\n');
            }
        }
        rendered
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ContextAssemblyPlanner;

impl ContextAssemblyPlanner {
    pub fn plan(
        &self,
        segments: &[ContextSegment],
        capabilities: &ProviderCacheCapabilities,
    ) -> Result<ContextAssemblyPlan, ContextAssemblyError> {
        validate_capabilities(capabilities)?;
        validate_segments(segments)?;

        let mut ordered = segments.to_vec();
        ordered.sort_by(|left, right| {
            trust_rank(left.trust_class)
                .cmp(&trust_rank(right.trust_class))
                .then_with(|| match capabilities.family {
                    ProviderCacheFamily::ImplicitCommonPrefix
                    | ProviderCacheFamily::ExplicitPrefixBreakpoints => stable_order(left, right),
                    ProviderCacheFamily::None | ProviderCacheFamily::CachedContextObject => {
                        attention_order(left, right)
                    }
                })
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut stable_bytes = 0_usize;
        let mut dynamic_bytes = 0_usize;
        let mut provider_visible_bytes = 0_usize;
        for segment in &ordered {
            let bytes = segment.provider_visible_len();
            provider_visible_bytes = provider_visible_bytes.saturating_add(bytes);
            if is_stable(segment.volatility) {
                stable_bytes = stable_bytes.saturating_add(bytes);
            } else {
                dynamic_bytes = dynamic_bytes.saturating_add(bytes);
            }
        }

        let cache_breakpoints =
            if capabilities.family == ProviderCacheFamily::ExplicitPrefixBreakpoints {
                explicit_breakpoints(&ordered, capabilities)
            } else {
                Vec::new()
            };

        Ok(ContextAssemblyPlan {
            ordered_segments: ordered,
            cache_breakpoints,
            stable_bytes,
            dynamic_bytes,
            provider_visible_bytes,
        })
    }
}

fn validate_capabilities(
    capabilities: &ProviderCacheCapabilities,
) -> Result<(), ContextAssemblyError> {
    if capabilities.family == ProviderCacheFamily::ExplicitPrefixBreakpoints
        && capabilities.maximum_breakpoints == 0
    {
        return Err(ContextAssemblyError::UnsupportedCacheGeometry(
            "explicit prefix caching declared with zero legal breakpoints".to_owned(),
        ));
    }
    if capabilities.family != ProviderCacheFamily::ExplicitPrefixBreakpoints
        && capabilities.maximum_breakpoints != 0
    {
        return Err(ContextAssemblyError::UnsupportedCacheGeometry(
            "breakpoints declared for a cache family that cannot expose them".to_owned(),
        ));
    }
    if capabilities.family == ProviderCacheFamily::CachedContextObject
        && capabilities.granularity != ProviderCacheGranularity::ContextObject
    {
        return Err(ContextAssemblyError::UnsupportedCacheGeometry(
            "cached-context objects require context-object granularity".to_owned(),
        ));
    }
    Ok(())
}

fn validate_segments(segments: &[ContextSegment]) -> Result<(), ContextAssemblyError> {
    let mut ids = BTreeSet::new();
    for segment in segments {
        if segment.id.trim().is_empty()
            || segment.content_hash.trim().is_empty()
            || segment.rendered_bytes.is_empty()
        {
            return Err(ContextAssemblyError::InvalidSegment(segment.id.clone()));
        }
        if !ids.insert(segment.id.as_str()) {
            return Err(ContextAssemblyError::DuplicateSegmentId(segment.id.clone()));
        }
        if segment.trust_class == ContextTrustClass::SystemAuthority
            && !matches!(
                segment.semantic_role,
                ContextSemanticRole::Constitution | ContextSemanticRole::ProjectOrientation
            )
        {
            return Err(ContextAssemblyError::TrustBoundaryViolation(
                segment.id.clone(),
            ));
        }
        if segment.trust_class == ContextTrustClass::SystemAuthority
            && matches!(
                segment.semantic_role,
                ContextSemanticRole::TaskEvidence
                    | ContextSemanticRole::DecisionCriticalEvidence
                    | ContextSemanticRole::IterationDelta
                    | ContextSemanticRole::UserObjective
            )
        {
            return Err(ContextAssemblyError::TrustBoundaryViolation(
                segment.id.clone(),
            ));
        }
    }
    Ok(())
}

fn explicit_breakpoints(
    ordered: &[ContextSegment],
    capabilities: &ProviderCacheCapabilities,
) -> Vec<usize> {
    let mut result = Vec::new();
    let mut prefix_bytes = 0_usize;
    let minimum = capabilities.minimum_cacheable_prefix_bytes.unwrap_or(0);
    for (index, segment) in ordered.iter().enumerate() {
        prefix_bytes = prefix_bytes.saturating_add(segment.provider_visible_len());
        let next_is_more_volatile = ordered
            .get(index + 1)
            .is_some_and(|next| next.volatility > segment.volatility);
        if is_stable(segment.volatility)
            && next_is_more_volatile
            && prefix_bytes >= minimum
            && result.len() < capabilities.maximum_breakpoints
        {
            result.push(index + 1);
        }
    }
    result
}

fn trust_rank(trust: ContextTrustClass) -> u8 {
    match trust {
        ContextTrustClass::SystemAuthority => 0,
        ContextTrustClass::UntrustedData => 1,
    }
}

fn stable_order(left: &ContextSegment, right: &ContextSegment) -> std::cmp::Ordering {
    left.volatility
        .cmp(&right.volatility)
        .then_with(|| left.reuse_scope.cmp(&right.reuse_scope))
        .then_with(|| role_rank(left.semantic_role).cmp(&role_rank(right.semantic_role)))
}

fn attention_order(left: &ContextSegment, right: &ContextSegment) -> std::cmp::Ordering {
    role_rank(left.semantic_role)
        .cmp(&role_rank(right.semantic_role))
        .then_with(|| left.volatility.cmp(&right.volatility))
        .then_with(|| left.reuse_scope.cmp(&right.reuse_scope))
}

fn role_rank(role: ContextSemanticRole) -> u8 {
    match role {
        ContextSemanticRole::Constitution => 0,
        ContextSemanticRole::ProjectOrientation => 1,
        ContextSemanticRole::TaskEvidence => 2,
        ContextSemanticRole::IterationDelta => 3,
        ContextSemanticRole::DecisionCriticalEvidence => 4,
        ContextSemanticRole::UserObjective => 5,
        ContextSemanticRole::OutputContract => 6,
    }
}

fn is_stable(volatility: ContextVolatility) -> bool {
    matches!(
        volatility,
        ContextVolatility::Immutable
            | ContextVolatility::ProjectStable
            | ContextVolatility::SnapshotStable
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextAssemblyError {
    InvalidSegment(String),
    DuplicateSegmentId(String),
    TrustBoundaryViolation(String),
    UnsupportedCacheGeometry(String),
}

impl fmt::Display for ContextAssemblyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSegment(id) => write!(formatter, "invalid context segment: {id}"),
            Self::DuplicateSegmentId(id) => {
                write!(formatter, "duplicate context segment identity: {id}")
            }
            Self::TrustBoundaryViolation(id) => write!(
                formatter,
                "untrusted/task context cannot enter system authority: {id}"
            ),
            Self::UnsupportedCacheGeometry(message) => {
                write!(formatter, "unsupported provider cache geometry: {message}")
            }
        }
    }
}

impl Error for ContextAssemblyError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(
        id: &str,
        role: ContextSemanticRole,
        trust: ContextTrustClass,
        reuse: ContextReuseScope,
        volatility: ContextVolatility,
        bytes: &str,
    ) -> ContextSegment {
        ContextSegment {
            id: id.to_owned(),
            semantic_role: role,
            trust_class: trust,
            reuse_scope: reuse,
            volatility,
            content_hash: format!("sha256:{id}"),
            token_estimate: 1,
            source_refs: Vec::new(),
            rendered_bytes: bytes.to_owned(),
        }
    }

    #[test]
    fn provider_capability_changes_geometry_not_semantics() {
        let segments = vec![
            segment(
                "dynamic",
                ContextSemanticRole::IterationDelta,
                ContextTrustClass::UntrustedData,
                ContextReuseScope::Iteration,
                ContextVolatility::IterationDynamic,
                "dynamic",
            ),
            segment(
                "stable",
                ContextSemanticRole::DecisionCriticalEvidence,
                ContextTrustClass::UntrustedData,
                ContextReuseScope::Snapshot,
                ContextVolatility::SnapshotStable,
                "stable",
            ),
        ];
        let planner = ContextAssemblyPlanner;
        let no_cache = planner
            .plan(&segments, &ProviderCacheCapabilities::no_cache())
            .expect("no-cache assembly");
        let prefix = planner
            .plan(
                &segments,
                &ProviderCacheCapabilities::delegated_claude_cli(),
            )
            .expect("prefix assembly");
        assert_eq!(no_cache.semantic_identities(), prefix.semantic_identities());
        assert_ne!(
            no_cache
                .ordered_segments
                .iter()
                .map(|segment| segment.id.as_str())
                .collect::<Vec<_>>(),
            prefix
                .ordered_segments
                .iter()
                .map(|segment| segment.id.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(prefix.ordered_segments[0].id, "stable");
    }

    #[test]
    fn repository_or_task_evidence_cannot_be_promoted_to_system_authority() {
        let planner = ContextAssemblyPlanner;
        let evidence = segment(
            "repo-source",
            ContextSemanticRole::DecisionCriticalEvidence,
            ContextTrustClass::SystemAuthority,
            ContextReuseScope::Snapshot,
            ContextVolatility::SnapshotStable,
            "source",
        );
        assert!(matches!(
            planner.plan(&[evidence], &ProviderCacheCapabilities::no_cache()),
            Err(ContextAssemblyError::TrustBoundaryViolation(_))
        ));
    }

    #[test]
    fn explicit_cache_requires_a_real_legal_breakpoint_capability() {
        let invalid = ProviderCacheCapabilities {
            family: ProviderCacheFamily::ExplicitPrefixBreakpoints,
            minimum_cacheable_prefix_bytes: Some(1),
            maximum_breakpoints: 0,
            supported_ttls: Vec::new(),
            cached_read_telemetry: false,
            cache_write_telemetry: false,
            stable_prefix_required: true,
            granularity: ProviderCacheGranularity::Block,
            cache_key_supported: false,
        };
        assert!(matches!(
            ContextAssemblyPlanner.plan(&[], &invalid),
            Err(ContextAssemblyError::UnsupportedCacheGeometry(_))
        ));
    }

    #[test]
    fn no_cache_plan_never_invents_cache_breakpoints_or_hits() {
        let plan = ContextAssemblyPlanner
            .plan(
                &[segment(
                    "task",
                    ContextSemanticRole::TaskEvidence,
                    ContextTrustClass::UntrustedData,
                    ContextReuseScope::Task,
                    ContextVolatility::TaskStable,
                    "task",
                )],
                &ProviderCacheCapabilities::no_cache(),
            )
            .expect("assembly");
        assert!(plan.cache_breakpoints.is_empty());
        assert_eq!(plan.provider_visible_bytes, 4);
    }
}
