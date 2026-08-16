use std::{
    collections::{BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};

use aer_provider::{
    AuthenticationMethod, CancellationSignal, NeverCancelled, ProviderAdapter, ProviderDescriptor,
    ProviderError, ProviderFailureClass, ProviderRequest, ProviderResponse, ProviderUsage, RetryPolicy,
    routing::{
        CircuitPolicy, DataSensitivity, EndpointCapabilities, EndpointHealth, EndpointProfile,
        EndpointTier, PrivacyPolicy, PricingSnapshot, RateLimitWindow, ResilientProviderPool,
        RetentionClass, RouteRequest, RouteRequirements, RouterPolicy, RoutingStrategy, UsageEstimate,
        UserQualityMode, route,
    },
};

struct BenchProvider {
    id: String,
    model: String,
    script: Mutex<VecDeque<Result<String, ProviderError>>>,
}

impl BenchProvider {
    fn new(
        id: &str,
        model: &str,
        script: impl IntoIterator<Item = Result<String, ProviderError>>,
    ) -> Self {
        Self {
            id: id.to_owned(),
            model: model.to_owned(),
            script: Mutex::new(script.into_iter().collect()),
        }
    }
}

impl ProviderAdapter for BenchProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: self.id.clone(),
            display_name: self.id.clone(),
            model: self.model.clone(),
            authentication: vec![AuthenticationMethod::TestOnly],
            production_ready: true,
        }
    }

    fn complete(
        &self,
        _request: &ProviderRequest,
        _cancellation: &dyn CancellationSignal,
    ) -> Result<ProviderResponse, ProviderError> {
        let output = self
            .script
            .lock()
            .expect("bench provider mutex poisoned")
            .pop_front()
            .expect("bench provider script exhausted")?;
        Ok(ProviderResponse {
            provider_id: self.id.clone(),
            model: self.model.clone(),
            output_text: output,
            usage: ProviderUsage::default(),
        })
    }
}

fn profile(id: &str, model: &str, price: u64, quality: u32, latency: u64) -> EndpointProfile {
    EndpointProfile {
        endpoint_id: id.to_owned(),
        provider: id.to_owned(),
        model_id: model.to_owned(),
        model_snapshot: Some("snapshot-1".to_owned()),
        region: Some("us".to_owned()),
        production_ready: true,
        credential_usable: true,
        capabilities: EndpointCapabilities {
            max_context_tokens: 128_000,
            max_output_tokens: 16_000,
            structured_output: true,
            tool_calls: true,
            parallel_tool_calls: true,
            streaming: true,
            multimodal_inputs: false,
            prompt_cache: true,
            reasoning_controls: true,
            cancellation: true,
        },
        privacy: PrivacyPolicy {
            maximum_sensitivity: DataSensitivity::Confidential,
            maximum_retention: RetentionClass::Ephemeral,
            allowed_regions: BTreeSet::from(["us".to_owned()]),
        },
        pricing: PricingSnapshot {
            observed_at_ms: 1_000,
            input_usd_micros_per_million_tokens: price,
            output_usd_micros_per_million_tokens: price,
            cached_input_usd_micros_per_million_tokens: price / 4,
        },
        capability_observed_at_ms: 1_000,
        capability_ttl_ms: 60_000,
        health: EndpointHealth::default(),
        rate_limit: RateLimitWindow::default(),
        tier: EndpointTier::General,
        verified_success_ppm: quality,
        p95_latency_ms: latency,
        architecture_risk_ppm: 10_000,
    }
}

fn route_request(mode: UserQualityMode) -> RouteRequest {
    RouteRequest {
        requirements: RouteRequirements {
            minimum_context_tokens: 32_000,
            minimum_output_tokens: 4_000,
            structured_output: true,
            tool_calls: true,
            parallel_tool_calls: true,
            sensitivity: DataSensitivity::Internal,
            maximum_retention: RetentionClass::Ephemeral,
            required_region: Some("us".to_owned()),
            pinned_model_snapshot: Some("snapshot-1".to_owned()),
            ..RouteRequirements::default()
        },
        usage: UsageEstimate {
            input_tokens: 20_000,
            output_tokens: 4_000,
            cached_input_tokens: 8_000,
        },
        remaining_cost_micros: 5_000_000,
        maximum_latency_ms: Some(10_000),
        mode,
        uncertainty_ppm: 0,
        allow_scout: false,
    }
}

fn provider_request() -> ProviderRequest {
    ProviderRequest {
        run_id: "provider-bench-run".to_owned(),
        attempt_id: "logical-call-1".to_owned(),
        instructions: "return fixture".to_owned(),
        input: "fixture".to_owned(),
        response_schema: None,
    }
}

#[test]
fn provider_bench_bounded_retry_then_failover_preserves_attempt_trace() {
    let mut pool = ResilientProviderPool::new(
        RetryPolicy::new(2, 0, 0).expect("retry policy"),
        RouterPolicy::new(700_000, 300_000, 1).expect("router policy"),
        CircuitPolicy::new(1, 5_000).expect("circuit policy"),
    );
    pool.add_provider(
        Arc::new(BenchProvider::new(
            "economy",
            "fixture-small",
            [
                Err(ProviderError::new(ProviderFailureClass::Timeout, "timeout-1")),
                Err(ProviderError::new(ProviderFailureClass::Timeout, "timeout-2")),
            ],
        )),
        profile("economy", "fixture-small", 1_000_000, 800_000, 500),
    )
    .expect("economy provider");
    pool.add_provider(
        Arc::new(BenchProvider::new(
            "quality",
            "fixture-large",
            [Ok("accepted".to_owned())],
        )),
        profile("quality", "fixture-large", 5_000_000, 970_000, 1_500),
    )
    .expect("quality provider");

    let result = pool
        .complete(
            &provider_request(),
            &route_request(UserQualityMode::Economy),
            2_000,
            &NeverCancelled,
        )
        .expect("bounded failover should succeed");

    assert_eq!(result.endpoint_id, "quality");
    assert_eq!(result.failovers, 1);
    assert_eq!(result.attempts.len(), 2);
    assert_eq!(result.attempts[0].gateway_attempts, 2);
    assert_eq!(result.attempts[1].gateway_attempts, 1);
    assert_eq!(result.attempts[1].routing_strategy, RoutingStrategy::Fallback);
}

#[test]
fn router_bench_measures_quality_cost_policy_tradeoff_without_model_names_in_policy() {
    let economy = profile("economy", "fixture-small", 1_000_000, 810_000, 600);
    let balanced = profile("balanced", "fixture-mid", 4_000_000, 930_000, 1_100);
    let quality = profile("quality", "fixture-large", 12_000_000, 985_000, 1_800);
    let candidates = [economy, balanced, quality];
    let policy = RouterPolicy::new(750_000, 300_000, 1).expect("policy");

    let cheap = route(
        &candidates,
        &route_request(UserQualityMode::Economy),
        policy,
        2_000,
        &BTreeSet::new(),
        false,
    )
    .expect("economy route");
    let max_quality = route(
        &candidates,
        &route_request(UserQualityMode::MaximumQuality),
        policy,
        2_000,
        &BTreeSet::new(),
        false,
    )
    .expect("quality route");

    assert_eq!(cheap.selected_endpoint_id, "economy");
    assert_eq!(max_quality.selected_endpoint_id, "quality");
    assert!(cheap.expected_cost_micros < max_quality.expected_cost_micros);
    assert_eq!(cheap.policy_version, max_quality.policy_version);
}
