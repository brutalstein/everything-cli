use aer_provider::context_assembly::{
    ContextAssemblyError, ContextAssemblyPlanner, ContextReuseScope, ContextSegment,
    ContextSemanticRole, ContextTrustClass, ContextVolatility, ProviderCacheCapabilities,
    ProviderCacheFamily, ProviderCacheGranularity, ProviderCacheTtl,
};

fn segment(
    id: &str,
    role: ContextSemanticRole,
    volatility: ContextVolatility,
    rendered: &str,
) -> ContextSegment {
    ContextSegment {
        id: id.to_owned(),
        semantic_role: role,
        trust_class: ContextTrustClass::UntrustedData,
        reuse_scope: match volatility {
            ContextVolatility::Immutable => ContextReuseScope::Global,
            ContextVolatility::ProjectStable => ContextReuseScope::Project,
            ContextVolatility::SnapshotStable => ContextReuseScope::Snapshot,
            ContextVolatility::TaskStable => ContextReuseScope::Task,
            ContextVolatility::IterationDynamic => ContextReuseScope::Iteration,
        },
        volatility,
        content_hash: format!("sha256:{id}"),
        token_estimate: 1,
        source_refs: vec![format!("audit:{id}:v1")],
        rendered_bytes: rendered.to_owned(),
    }
}

#[test]
fn audit_metadata_churn_cannot_change_provider_visible_bytes() {
    let planner = ContextAssemblyPlanner;
    let original = segment(
        "source",
        ContextSemanticRole::DecisionCriticalEvidence,
        ContextVolatility::SnapshotStable,
        "exact source\n",
    );
    let mut churned = original.clone();
    churned.source_refs = vec!["audit:completely-different".to_owned()];
    let first = planner
        .plan(&[original], &ProviderCacheCapabilities::no_cache())
        .expect("first");
    let second = planner
        .plan(&[churned], &ProviderCacheCapabilities::no_cache())
        .expect("second");
    assert_eq!(first.render(), second.render());
    assert_eq!(first.provider_visible_bytes, second.provider_visible_bytes);
}

#[test]
fn prefix_cache_orders_stable_semantics_before_iteration_delta() {
    let planner = ContextAssemblyPlanner;
    let dynamic = segment(
        "delta",
        ContextSemanticRole::IterationDelta,
        ContextVolatility::IterationDynamic,
        "latest diff\n",
    );
    let stable = segment(
        "source",
        ContextSemanticRole::DecisionCriticalEvidence,
        ContextVolatility::SnapshotStable,
        "exact source\n",
    );
    let plan = planner
        .plan(
            &[dynamic, stable],
            &ProviderCacheCapabilities::delegated_claude_cli(),
        )
        .expect("prefix plan");
    assert_eq!(plan.ordered_segments[0].id, "source");
    assert_eq!(plan.ordered_segments[1].id, "delta");
    assert!(plan.stable_bytes > 0);
    assert!(plan.dynamic_bytes > 0);
    assert!(
        plan.cache_breakpoints.is_empty(),
        "CLI exposes no legal AER breakpoint"
    );
}

#[test]
fn explicit_cache_uses_only_declared_legal_boundaries() {
    let planner = ContextAssemblyPlanner;
    let capabilities = ProviderCacheCapabilities {
        family: ProviderCacheFamily::ExplicitPrefixBreakpoints,
        minimum_cacheable_prefix_bytes: Some(4),
        maximum_breakpoints: 1,
        supported_ttls: vec![ProviderCacheTtl::FiveMinutes],
        cached_read_telemetry: true,
        cache_write_telemetry: true,
        stable_prefix_required: true,
        granularity: ProviderCacheGranularity::Block,
        cache_key_supported: false,
    };
    let plan = planner
        .plan(
            &[
                segment(
                    "stable",
                    ContextSemanticRole::TaskEvidence,
                    ContextVolatility::SnapshotStable,
                    "stable bytes\n",
                ),
                segment(
                    "dynamic",
                    ContextSemanticRole::IterationDelta,
                    ContextVolatility::IterationDynamic,
                    "dynamic bytes\n",
                ),
            ],
            &capabilities,
        )
        .expect("explicit cache plan");
    assert_eq!(plan.cache_breakpoints, vec![1]);

    let illegal = ProviderCacheCapabilities {
        family: ProviderCacheFamily::None,
        maximum_breakpoints: 1,
        ..ProviderCacheCapabilities::no_cache()
    };
    assert!(matches!(
        planner.plan(&[], &illegal),
        Err(ContextAssemblyError::UnsupportedCacheGeometry(_))
    ));
}
