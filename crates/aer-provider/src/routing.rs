use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

use crate::{
    CancellationSignal, GatewayError, ProviderAdapter, ProviderDescriptor, ProviderFailureClass,
    ProviderGateway, ProviderRequest, ProviderResponse, RetryPolicy,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DataSensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RetentionClass {
    None,
    Ephemeral,
    Standard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointTier {
    Scout,
    General,
    HighCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserQualityMode {
    Economy,
    Balanced,
    MaximumQuality,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthState {
    Healthy,
    Degraded,
    RateLimited,
    OpenCircuit,
    Unavailable,
    DisabledByPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointCapabilities {
    pub max_context_tokens: u64,
    pub max_output_tokens: u64,
    pub structured_output: bool,
    pub tool_calls: bool,
    pub parallel_tool_calls: bool,
    pub streaming: bool,
    pub multimodal_inputs: bool,
    pub prompt_cache: bool,
    pub reasoning_controls: bool,
    pub cancellation: bool,
}

impl Default for EndpointCapabilities {
    fn default() -> Self {
        Self {
            max_context_tokens: 8_192,
            max_output_tokens: 2_048,
            structured_output: false,
            tool_calls: false,
            parallel_tool_calls: false,
            streaming: false,
            multimodal_inputs: false,
            prompt_cache: false,
            reasoning_controls: false,
            cancellation: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacyPolicy {
    pub maximum_sensitivity: DataSensitivity,
    pub maximum_retention: RetentionClass,
    pub allowed_regions: BTreeSet<String>,
}

impl Default for PrivacyPolicy {
    fn default() -> Self {
        Self {
            maximum_sensitivity: DataSensitivity::Internal,
            maximum_retention: RetentionClass::Standard,
            allowed_regions: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PricingSnapshot {
    pub observed_at_ms: u64,
    pub input_usd_micros_per_million_tokens: u64,
    pub output_usd_micros_per_million_tokens: u64,
    pub cached_input_usd_micros_per_million_tokens: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UsageEstimate {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
}

impl PricingSnapshot {
    pub fn estimate_cost_micros(self, usage: UsageEstimate) -> Result<u64, RoutingError> {
        let input = cost_component(usage.input_tokens, self.input_usd_micros_per_million_tokens)?;
        let output = cost_component(
            usage.output_tokens,
            self.output_usd_micros_per_million_tokens,
        )?;
        let cached = cost_component(
            usage.cached_input_tokens,
            self.cached_input_usd_micros_per_million_tokens,
        )?;
        input
            .checked_add(output)
            .and_then(|value| value.checked_add(cached))
            .ok_or(RoutingError::CostOverflow)
    }
}

fn cost_component(tokens: u64, micros_per_million: u64) -> Result<u64, RoutingError> {
    let product = u128::from(tokens)
        .checked_mul(u128::from(micros_per_million))
        .ok_or(RoutingError::CostOverflow)?;
    let rounded = if product == 0 {
        0
    } else {
        product
            .checked_add(999_999)
            .ok_or(RoutingError::CostOverflow)?
            / 1_000_000
    };
    u64::try_from(rounded).map_err(|_| RoutingError::CostOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CircuitPolicy {
    pub transient_failure_threshold: u32,
    pub cooldown_ms: u64,
}

impl CircuitPolicy {
    pub fn new(transient_failure_threshold: u32, cooldown_ms: u64) -> Result<Self, RoutingError> {
        if transient_failure_threshold == 0 {
            return Err(RoutingError::InvalidPolicy(
                "circuit failure threshold must be greater than zero",
            ));
        }
        Ok(Self {
            transient_failure_threshold,
            cooldown_ms,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointHealth {
    pub state: HealthState,
    pub consecutive_transient_failures: u32,
    pub circuit_open_until_ms: Option<u64>,
    pub rate_limited_until_ms: Option<u64>,
}

impl Default for EndpointHealth {
    fn default() -> Self {
        Self {
            state: HealthState::Healthy,
            consecutive_transient_failures: 0,
            circuit_open_until_ms: None,
            rate_limited_until_ms: None,
        }
    }
}

impl EndpointHealth {
    #[must_use]
    pub fn effective_state(&self, now_ms: u64) -> HealthState {
        if self.state == HealthState::DisabledByPolicy || self.state == HealthState::Unavailable {
            return self.state;
        }
        if self
            .rate_limited_until_ms
            .is_some_and(|until| now_ms < until)
        {
            return HealthState::RateLimited;
        }
        if self
            .circuit_open_until_ms
            .is_some_and(|until| now_ms < until)
        {
            return HealthState::OpenCircuit;
        }
        match self.state {
            HealthState::RateLimited | HealthState::OpenCircuit => HealthState::Degraded,
            other => other,
        }
    }

    pub fn record_success(&mut self) {
        self.state = HealthState::Healthy;
        self.consecutive_transient_failures = 0;
        self.circuit_open_until_ms = None;
        self.rate_limited_until_ms = None;
    }

    pub fn record_failure(
        &mut self,
        class: ProviderFailureClass,
        retry_after_ms: Option<u64>,
        now_ms: u64,
        policy: CircuitPolicy,
    ) {
        if class == ProviderFailureClass::RateLimited {
            self.state = HealthState::RateLimited;
            self.rate_limited_until_ms = retry_after_ms.map(|delay| now_ms.saturating_add(delay));
            return;
        }
        if class.opens_circuit() {
            self.consecutive_transient_failures =
                self.consecutive_transient_failures.saturating_add(1);
            if self.consecutive_transient_failures >= policy.transient_failure_threshold {
                self.state = HealthState::OpenCircuit;
                self.circuit_open_until_ms = Some(now_ms.saturating_add(policy.cooldown_ms));
            } else {
                self.state = HealthState::Degraded;
            }
            return;
        }
        if matches!(
            class,
            ProviderFailureClass::Authentication
                | ProviderFailureClass::Authorization
                | ProviderFailureClass::QuotaExhausted
        ) {
            self.state = HealthState::Unavailable;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitWindow {
    pub remaining_requests: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub reset_at_ms: Option<u64>,
}

impl Default for RateLimitWindow {
    fn default() -> Self {
        Self {
            remaining_requests: None,
            remaining_tokens: None,
            reset_at_ms: None,
        }
    }
}

impl RateLimitWindow {
    fn maybe_reset(&mut self, now_ms: u64) {
        if self.reset_at_ms.is_some_and(|reset| now_ms >= reset) {
            self.remaining_requests = None;
            self.remaining_tokens = None;
            self.reset_at_ms = None;
        }
    }

    pub fn reserve(&mut self, tokens: u64, now_ms: u64) -> Result<(), RoutingError> {
        self.maybe_reset(now_ms);
        if self.remaining_requests == Some(0) {
            return Err(RoutingError::RateLimitReservation);
        }
        if self
            .remaining_tokens
            .is_some_and(|remaining| remaining < tokens)
        {
            return Err(RoutingError::RateLimitReservation);
        }
        if let Some(remaining) = &mut self.remaining_requests {
            *remaining -= 1;
        }
        if let Some(remaining) = &mut self.remaining_tokens {
            *remaining -= tokens;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointProfile {
    pub endpoint_id: String,
    pub provider: String,
    pub model_id: String,
    pub model_snapshot: Option<String>,
    pub region: Option<String>,
    pub production_ready: bool,
    pub credential_usable: bool,
    pub capabilities: EndpointCapabilities,
    pub privacy: PrivacyPolicy,
    pub pricing: PricingSnapshot,
    pub capability_observed_at_ms: u64,
    pub capability_ttl_ms: u64,
    pub health: EndpointHealth,
    pub rate_limit: RateLimitWindow,
    pub tier: EndpointTier,
    pub verified_success_ppm: u32,
    pub p95_latency_ms: u64,
    pub architecture_risk_ppm: u32,
}

impl EndpointProfile {
    #[must_use]
    pub fn capabilities_are_fresh(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.capability_observed_at_ms) <= self.capability_ttl_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteRequirements {
    pub minimum_context_tokens: u64,
    pub minimum_output_tokens: u64,
    pub structured_output: bool,
    pub tool_calls: bool,
    pub parallel_tool_calls: bool,
    pub multimodal_inputs: bool,
    pub sensitivity: DataSensitivity,
    pub maximum_retention: RetentionClass,
    pub required_region: Option<String>,
    pub pinned_model_snapshot: Option<String>,
}

impl Default for RouteRequirements {
    fn default() -> Self {
        Self {
            minimum_context_tokens: 0,
            minimum_output_tokens: 0,
            structured_output: false,
            tool_calls: false,
            parallel_tool_calls: false,
            multimodal_inputs: false,
            sensitivity: DataSensitivity::Internal,
            maximum_retention: RetentionClass::Standard,
            required_region: None,
            pinned_model_snapshot: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteRequest {
    pub requirements: RouteRequirements,
    pub usage: UsageEstimate,
    pub remaining_cost_micros: u64,
    pub maximum_latency_ms: Option<u64>,
    pub mode: UserQualityMode,
    pub uncertainty_ppm: u32,
    pub allow_scout: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouterPolicy {
    pub minimum_verified_success_ppm: u32,
    pub scout_uncertainty_threshold_ppm: u32,
    pub max_failovers: u32,
}

impl RouterPolicy {
    pub fn new(
        minimum_verified_success_ppm: u32,
        scout_uncertainty_threshold_ppm: u32,
        max_failovers: u32,
    ) -> Result<Self, RoutingError> {
        if minimum_verified_success_ppm > 1_000_000 || scout_uncertainty_threshold_ppm > 1_000_000 {
            return Err(RoutingError::InvalidPolicy(
                "probability values must be in 0..=1_000_000 ppm",
            ));
        }
        Ok(Self {
            minimum_verified_success_ppm,
            scout_uncertainty_threshold_ppm,
            max_failovers,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RejectionReason {
    NotProductionReady,
    CredentialUnavailable,
    StaleCapabilities,
    Health(HealthState),
    InsufficientContext,
    InsufficientOutput,
    MissingStructuredOutput,
    MissingToolCalls,
    MissingParallelToolCalls,
    MissingMultimodal,
    SensitivityPolicy,
    RetentionPolicy,
    RegionPolicy,
    SnapshotMismatch,
    CostBudget,
    LatencyBudget,
    BelowQualityFloor,
    ExcludedAfterFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRejection {
    pub endpoint_id: String,
    pub reason: RejectionReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingStrategy {
    Direct,
    ScoutThenRoute,
    Fallback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingDecision {
    pub policy_version: &'static str,
    pub selected_endpoint_id: String,
    pub strategy: RoutingStrategy,
    pub expected_cost_micros: u64,
    pub eligible_endpoint_ids: Vec<String>,
    pub rejections: Vec<CandidateRejection>,
}

pub fn route(
    candidates: &[EndpointProfile],
    request: &RouteRequest,
    policy: RouterPolicy,
    now_ms: u64,
    excluded: &BTreeSet<String>,
    is_fallback: bool,
) -> Result<RoutingDecision, RoutingError> {
    let mut eligible = Vec::new();
    let mut rejections = Vec::new();
    for candidate in candidates {
        match eligibility(candidate, request, policy, now_ms, excluded) {
            Ok(cost) => eligible.push((candidate, cost)),
            Err(reason) => rejections.push(CandidateRejection {
                endpoint_id: candidate.endpoint_id.clone(),
                reason,
            }),
        }
    }
    if eligible.is_empty() {
        return Err(RoutingError::NoEligibleEndpoint { rejections });
    }

    let wants_scout = !is_fallback
        && request.allow_scout
        && request.uncertainty_ppm >= policy.scout_uncertainty_threshold_ppm;
    if wants_scout
        && eligible
            .iter()
            .any(|(candidate, _)| candidate.tier == EndpointTier::Scout)
    {
        eligible.retain(|(candidate, _)| candidate.tier == EndpointTier::Scout);
    }

    eligible.sort_by(|(left, left_cost), (right, right_cost)| {
        compare_candidates(left, *left_cost, right, *right_cost, request.mode)
    });
    let (selected, expected_cost_micros) = eligible[0];
    let strategy = if is_fallback {
        RoutingStrategy::Fallback
    } else if wants_scout && selected.tier == EndpointTier::Scout {
        RoutingStrategy::ScoutThenRoute
    } else {
        RoutingStrategy::Direct
    };
    Ok(RoutingDecision {
        policy_version: "router-v1",
        selected_endpoint_id: selected.endpoint_id.clone(),
        strategy,
        expected_cost_micros,
        eligible_endpoint_ids: eligible
            .iter()
            .map(|(candidate, _)| candidate.endpoint_id.clone())
            .collect(),
        rejections,
    })
}

fn eligibility(
    candidate: &EndpointProfile,
    request: &RouteRequest,
    policy: RouterPolicy,
    now_ms: u64,
    excluded: &BTreeSet<String>,
) -> Result<u64, RejectionReason> {
    if excluded.contains(&candidate.endpoint_id) {
        return Err(RejectionReason::ExcludedAfterFailure);
    }
    if !candidate.production_ready {
        return Err(RejectionReason::NotProductionReady);
    }
    if !candidate.credential_usable {
        return Err(RejectionReason::CredentialUnavailable);
    }
    if !candidate.capabilities_are_fresh(now_ms) {
        return Err(RejectionReason::StaleCapabilities);
    }
    let health = candidate.health.effective_state(now_ms);
    if !matches!(health, HealthState::Healthy | HealthState::Degraded) {
        return Err(RejectionReason::Health(health));
    }
    if candidate.capabilities.max_context_tokens < request.requirements.minimum_context_tokens {
        return Err(RejectionReason::InsufficientContext);
    }
    if candidate.capabilities.max_output_tokens < request.requirements.minimum_output_tokens {
        return Err(RejectionReason::InsufficientOutput);
    }
    if request.requirements.structured_output && !candidate.capabilities.structured_output {
        return Err(RejectionReason::MissingStructuredOutput);
    }
    if request.requirements.tool_calls && !candidate.capabilities.tool_calls {
        return Err(RejectionReason::MissingToolCalls);
    }
    if request.requirements.parallel_tool_calls && !candidate.capabilities.parallel_tool_calls {
        return Err(RejectionReason::MissingParallelToolCalls);
    }
    if request.requirements.multimodal_inputs && !candidate.capabilities.multimodal_inputs {
        return Err(RejectionReason::MissingMultimodal);
    }
    if request.requirements.sensitivity > candidate.privacy.maximum_sensitivity {
        return Err(RejectionReason::SensitivityPolicy);
    }
    if candidate.privacy.maximum_retention > request.requirements.maximum_retention {
        return Err(RejectionReason::RetentionPolicy);
    }
    if let Some(required_region) = &request.requirements.required_region {
        if candidate.region.as_ref() != Some(required_region)
            || (!candidate.privacy.allowed_regions.is_empty()
                && !candidate.privacy.allowed_regions.contains(required_region))
        {
            return Err(RejectionReason::RegionPolicy);
        }
    }
    if let Some(required_snapshot) = &request.requirements.pinned_model_snapshot {
        if candidate.model_snapshot.as_ref() != Some(required_snapshot) {
            return Err(RejectionReason::SnapshotMismatch);
        }
    }
    if candidate.verified_success_ppm < policy.minimum_verified_success_ppm {
        return Err(RejectionReason::BelowQualityFloor);
    }
    if request
        .maximum_latency_ms
        .is_some_and(|limit| candidate.p95_latency_ms > limit)
    {
        return Err(RejectionReason::LatencyBudget);
    }
    let cost = candidate
        .pricing
        .estimate_cost_micros(request.usage)
        .map_err(|_| RejectionReason::CostBudget)?;
    if cost > request.remaining_cost_micros {
        return Err(RejectionReason::CostBudget);
    }
    Ok(cost)
}

fn compare_candidates(
    left: &EndpointProfile,
    left_cost: u64,
    right: &EndpointProfile,
    right_cost: u64,
    mode: UserQualityMode,
) -> Ordering {
    let ordering = match mode {
        UserQualityMode::Economy => left_cost
            .cmp(&right_cost)
            .then_with(|| right.verified_success_ppm.cmp(&left.verified_success_ppm))
            .then_with(|| left.p95_latency_ms.cmp(&right.p95_latency_ms)),
        UserQualityMode::Balanced => balanced_score(right, right_cost)
            .cmp(&balanced_score(left, left_cost))
            .then_with(|| right.verified_success_ppm.cmp(&left.verified_success_ppm))
            .then_with(|| left_cost.cmp(&right_cost)),
        UserQualityMode::MaximumQuality => right
            .verified_success_ppm
            .cmp(&left.verified_success_ppm)
            .then_with(|| left.architecture_risk_ppm.cmp(&right.architecture_risk_ppm))
            .then_with(|| left.p95_latency_ms.cmp(&right.p95_latency_ms))
            .then_with(|| left_cost.cmp(&right_cost)),
    };
    ordering.then_with(|| left.endpoint_id.cmp(&right.endpoint_id))
}

fn balanced_score(candidate: &EndpointProfile, cost_micros: u64) -> i128 {
    i128::from(candidate.verified_success_ppm) * 10_000
        - i128::from(candidate.architecture_risk_ppm) * 2_000
        - i128::from(candidate.p95_latency_ms) * 50
        - i128::from(cost_micros) * 100
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderAttemptRecord {
    pub endpoint_id: String,
    pub routing_strategy: RoutingStrategy,
    pub expected_cost_micros: u64,
    pub gateway_attempts: u32,
    pub outcome: AttemptOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AttemptOutcome {
    Success,
    Failed(ProviderFailureClass),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResilientResponse {
    pub response: ProviderResponse,
    pub endpoint_id: String,
    pub failovers: u32,
    pub attempts: Vec<ProviderAttemptRecord>,
}

struct ManagedProvider {
    adapter: Arc<dyn ProviderAdapter>,
    profile: Mutex<EndpointProfile>,
}

pub struct ResilientProviderPool {
    providers: BTreeMap<String, ManagedProvider>,
    retry_policy: RetryPolicy,
    router_policy: RouterPolicy,
    circuit_policy: CircuitPolicy,
}

impl ResilientProviderPool {
    pub fn new(
        retry_policy: RetryPolicy,
        router_policy: RouterPolicy,
        circuit_policy: CircuitPolicy,
    ) -> Self {
        Self {
            providers: BTreeMap::new(),
            retry_policy,
            router_policy,
            circuit_policy,
        }
    }

    pub fn add_provider(
        &mut self,
        adapter: Arc<dyn ProviderAdapter>,
        profile: EndpointProfile,
    ) -> Result<(), RoutingError> {
        let descriptor = adapter.descriptor();
        validate_profile_binding(&descriptor, &profile)?;
        if self.providers.contains_key(&profile.endpoint_id) {
            return Err(RoutingError::DuplicateEndpoint(profile.endpoint_id));
        }
        self.providers.insert(
            profile.endpoint_id.clone(),
            ManagedProvider {
                adapter,
                profile: Mutex::new(profile),
            },
        );
        Ok(())
    }

    pub fn profile(&self, endpoint_id: &str) -> Option<EndpointProfile> {
        self.providers.get(endpoint_id).map(|managed| {
            managed
                .profile
                .lock()
                .expect("provider profile mutex poisoned")
                .clone()
        })
    }

    pub fn complete(
        &self,
        provider_request: &ProviderRequest,
        route_request: &RouteRequest,
        now_ms: u64,
        cancellation: &dyn CancellationSignal,
    ) -> Result<ResilientResponse, FleetError> {
        let mut excluded = BTreeSet::new();
        let mut attempts = Vec::new();
        let mut failovers = 0_u32;
        loop {
            let profiles = self
                .providers
                .values()
                .map(|managed| {
                    managed
                        .profile
                        .lock()
                        .expect("provider profile mutex poisoned")
                        .clone()
                })
                .collect::<Vec<_>>();
            let decision = route(
                &profiles,
                route_request,
                self.router_policy,
                now_ms,
                &excluded,
                failovers > 0,
            )
            .map_err(|source| FleetError::Routing {
                source,
                attempts: attempts.clone(),
            })?;
            let managed = self
                .providers
                .get(&decision.selected_endpoint_id)
                .ok_or_else(|| {
                    FleetError::Integrity("router selected unknown endpoint".to_owned())
                })?;
            let total_tokens = route_request
                .usage
                .input_tokens
                .saturating_add(route_request.usage.output_tokens)
                .saturating_add(route_request.usage.cached_input_tokens);
            {
                let mut profile = managed
                    .profile
                    .lock()
                    .expect("provider profile mutex poisoned");
                profile
                    .rate_limit
                    .reserve(total_tokens, now_ms)
                    .map_err(|source| FleetError::Routing {
                        source,
                        attempts: attempts.clone(),
                    })?;
            }
            let gateway = ProviderGateway::new(Arc::clone(&managed.adapter), self.retry_policy);
            match gateway.complete(provider_request, cancellation) {
                Ok(result) => {
                    managed
                        .profile
                        .lock()
                        .expect("provider profile mutex poisoned")
                        .health
                        .record_success();
                    attempts.push(ProviderAttemptRecord {
                        endpoint_id: decision.selected_endpoint_id.clone(),
                        routing_strategy: decision.strategy,
                        expected_cost_micros: decision.expected_cost_micros,
                        gateway_attempts: result.attempts,
                        outcome: AttemptOutcome::Success,
                    });
                    return Ok(ResilientResponse {
                        response: result.response,
                        endpoint_id: decision.selected_endpoint_id,
                        failovers,
                        attempts,
                    });
                }
                Err(GatewayError::Provider(error)) => {
                    managed
                        .profile
                        .lock()
                        .expect("provider profile mutex poisoned")
                        .health
                        .record_failure(
                            error.class,
                            error.retry_after_ms,
                            now_ms,
                            self.circuit_policy,
                        );
                    attempts.push(ProviderAttemptRecord {
                        endpoint_id: decision.selected_endpoint_id.clone(),
                        routing_strategy: decision.strategy,
                        expected_cost_micros: decision.expected_cost_micros,
                        gateway_attempts: if error.retryable() {
                            self.retry_policy.max_attempts
                        } else {
                            1
                        },
                        outcome: AttemptOutcome::Failed(error.class),
                    });
                    if error.class.fallback_eligible()
                        && failovers < self.router_policy.max_failovers
                    {
                        excluded.insert(decision.selected_endpoint_id);
                        failovers = failovers.saturating_add(1);
                        continue;
                    }
                    return Err(FleetError::Provider { error, attempts });
                }
                Err(error) => {
                    return Err(FleetError::Gateway { error, attempts });
                }
            }
        }
    }
}

fn validate_profile_binding(
    descriptor: &ProviderDescriptor,
    profile: &EndpointProfile,
) -> Result<(), RoutingError> {
    if descriptor.id != profile.provider || descriptor.model != profile.model_id {
        return Err(RoutingError::DescriptorProfileMismatch {
            descriptor_provider: descriptor.id.clone(),
            descriptor_model: descriptor.model.clone(),
            profile_provider: profile.provider.clone(),
            profile_model: profile.model_id.clone(),
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingError {
    InvalidPolicy(&'static str),
    CostOverflow,
    RateLimitReservation,
    DuplicateEndpoint(String),
    DescriptorProfileMismatch {
        descriptor_provider: String,
        descriptor_model: String,
        profile_provider: String,
        profile_model: String,
    },
    NoEligibleEndpoint {
        rejections: Vec<CandidateRejection>,
    },
}

impl fmt::Display for RoutingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy(message) => write!(formatter, "invalid routing policy: {message}"),
            Self::CostOverflow => formatter.write_str("provider cost estimate overflow"),
            Self::RateLimitReservation => formatter.write_str("provider quota reservation refused"),
            Self::DuplicateEndpoint(endpoint) => {
                write!(formatter, "duplicate endpoint: {endpoint}")
            }
            Self::DescriptorProfileMismatch { .. } => {
                formatter.write_str("provider descriptor does not match routing profile")
            }
            Self::NoEligibleEndpoint { rejections } => {
                write!(
                    formatter,
                    "no eligible provider endpoint ({} rejections)",
                    rejections.len()
                )
            }
        }
    }
}

impl Error for RoutingError {}

#[derive(Debug)]
pub enum FleetError {
    Routing {
        source: RoutingError,
        attempts: Vec<ProviderAttemptRecord>,
    },
    Provider {
        error: crate::ProviderError,
        attempts: Vec<ProviderAttemptRecord>,
    },
    Gateway {
        error: GatewayError,
        attempts: Vec<ProviderAttemptRecord>,
    },
    Integrity(String),
}

impl fmt::Display for FleetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Routing { source, .. } => source.fmt(formatter),
            Self::Provider { error, .. } => error.fmt(formatter),
            Self::Gateway { error, .. } => error.fmt(formatter),
            Self::Integrity(message) => write!(formatter, "provider fleet integrity: {message}"),
        }
    }
}

impl Error for FleetError {}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex};

    use super::*;
    use crate::{
        AuthenticationMethod, NeverCancelled, ProviderError, ProviderResponse, ProviderUsage,
    };

    struct FixtureProvider {
        id: String,
        model: String,
        script: Mutex<VecDeque<Result<String, ProviderError>>>,
    }

    impl FixtureProvider {
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

    impl ProviderAdapter for FixtureProvider {
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
                .expect("fixture script mutex poisoned")
                .pop_front()
                .expect("fixture script exhausted")?;
            Ok(ProviderResponse {
                provider_id: self.id.clone(),
                model: self.model.clone(),
                output_text: output,
                usage: ProviderUsage::default(),
            })
        }
    }

    fn profile(id: &str, model: &str, cost: u64, quality: u32) -> EndpointProfile {
        EndpointProfile {
            endpoint_id: id.to_owned(),
            provider: id.to_owned(),
            model_id: model.to_owned(),
            model_snapshot: Some("snapshot-1".to_owned()),
            region: Some("us".to_owned()),
            production_ready: true,
            credential_usable: true,
            capabilities: EndpointCapabilities {
                max_context_tokens: 64_000,
                max_output_tokens: 8_000,
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
                input_usd_micros_per_million_tokens: cost,
                output_usd_micros_per_million_tokens: cost,
                cached_input_usd_micros_per_million_tokens: cost / 2,
            },
            capability_observed_at_ms: 1_000,
            capability_ttl_ms: 10_000,
            health: EndpointHealth::default(),
            rate_limit: RateLimitWindow::default(),
            tier: EndpointTier::General,
            verified_success_ppm: quality,
            p95_latency_ms: 1_000,
            architecture_risk_ppm: 20_000,
        }
    }

    fn route_request(mode: UserQualityMode) -> RouteRequest {
        RouteRequest {
            requirements: RouteRequirements {
                minimum_context_tokens: 8_000,
                minimum_output_tokens: 1_000,
                structured_output: true,
                tool_calls: true,
                sensitivity: DataSensitivity::Internal,
                maximum_retention: RetentionClass::Ephemeral,
                required_region: Some("us".to_owned()),
                pinned_model_snapshot: Some("snapshot-1".to_owned()),
                ..RouteRequirements::default()
            },
            usage: UsageEstimate {
                input_tokens: 10_000,
                output_tokens: 2_000,
                cached_input_tokens: 0,
            },
            remaining_cost_micros: 1_000_000,
            maximum_latency_ms: Some(5_000),
            mode,
            uncertainty_ppm: 0,
            allow_scout: false,
        }
    }

    fn provider_request() -> ProviderRequest {
        ProviderRequest {
            run_id: "run".to_owned(),
            attempt_id: "attempt".to_owned(),
            instructions: "test".to_owned(),
            input: "input".to_owned(),
            response_schema: None,
        }
    }

    #[test]
    fn exact_integer_cost_rounds_up_and_never_under_accounts() {
        let pricing = PricingSnapshot {
            input_usd_micros_per_million_tokens: 5_000_000,
            output_usd_micros_per_million_tokens: 15_000_000,
            ..PricingSnapshot::default()
        };
        let cost = pricing
            .estimate_cost_micros(UsageEstimate {
                input_tokens: 1,
                output_tokens: 1,
                cached_input_tokens: 0,
            })
            .expect("cost");
        assert_eq!(cost, 20);
    }

    #[test]
    fn stale_privacy_health_and_budget_constraints_fail_closed() {
        let mut stale = profile("stale", "m", 10_000, 900_000);
        stale.capability_ttl_ms = 1;
        let mut unhealthy = profile("down", "m", 10_000, 900_000);
        unhealthy.health.state = HealthState::Unavailable;
        let expensive = profile("expensive", "m", 100_000_000, 900_000);
        let error = route(
            &[stale, unhealthy, expensive],
            &route_request(UserQualityMode::Balanced),
            RouterPolicy::new(500_000, 300_000, 1).expect("policy"),
            2_000,
            &BTreeSet::new(),
            false,
        )
        .expect_err("all candidates must be rejected");
        let RoutingError::NoEligibleEndpoint { rejections } = error else {
            panic!("unexpected error");
        };
        assert_eq!(rejections.len(), 3);
    }

    #[test]
    fn economy_and_maximum_quality_make_different_inspectable_choices() {
        let cheap = profile("cheap", "m1", 1_000_000, 750_000);
        let strong = profile("strong", "m2", 20_000_000, 980_000);
        let policy = RouterPolicy::new(700_000, 300_000, 1).expect("policy");
        let economy = route(
            &[cheap.clone(), strong.clone()],
            &route_request(UserQualityMode::Economy),
            policy,
            1_500,
            &BTreeSet::new(),
            false,
        )
        .expect("economy route");
        let quality = route(
            &[cheap, strong],
            &route_request(UserQualityMode::MaximumQuality),
            policy,
            1_500,
            &BTreeSet::new(),
            false,
        )
        .expect("quality route");
        assert_eq!(economy.selected_endpoint_id, "cheap");
        assert_eq!(quality.selected_endpoint_id, "strong");
        assert_eq!(quality.policy_version, "router-v1");
    }

    #[test]
    fn high_uncertainty_routes_through_eligible_scout() {
        let mut scout = profile("scout", "m1", 1_000_000, 800_000);
        scout.tier = EndpointTier::Scout;
        let strong = profile("strong", "m2", 2_000_000, 950_000);
        let mut request = route_request(UserQualityMode::Balanced);
        request.allow_scout = true;
        request.uncertainty_ppm = 500_000;
        let decision = route(
            &[scout, strong],
            &request,
            RouterPolicy::new(700_000, 300_000, 1).expect("policy"),
            1_500,
            &BTreeSet::new(),
            false,
        )
        .expect("scout route");
        assert_eq!(decision.selected_endpoint_id, "scout");
        assert_eq!(decision.strategy, RoutingStrategy::ScoutThenRoute);
    }

    #[test]
    fn circuit_breaker_opens_after_bounded_transient_failures_and_recovers_on_success() {
        let policy = CircuitPolicy::new(2, 1_000).expect("policy");
        let mut health = EndpointHealth::default();
        health.record_failure(ProviderFailureClass::Timeout, None, 100, policy);
        assert_eq!(health.effective_state(100), HealthState::Degraded);
        health.record_failure(ProviderFailureClass::Connection, None, 200, policy);
        assert_eq!(health.effective_state(200), HealthState::OpenCircuit);
        assert_eq!(health.effective_state(1_200), HealthState::Degraded);
        health.record_success();
        assert_eq!(health.effective_state(1_200), HealthState::Healthy);
    }

    #[test]
    fn rate_limit_reservation_prevents_local_oversubscription() {
        let mut window = RateLimitWindow {
            remaining_requests: Some(1),
            remaining_tokens: Some(100),
            reset_at_ms: Some(2_000),
        };
        window.reserve(80, 1_000).expect("first reservation");
        assert!(window.reserve(1, 1_000).is_err());
    }

    #[test]
    fn exhausted_transient_provider_fails_over_once_to_next_eligible_endpoint() {
        let retry = RetryPolicy::new(2, 0, 0).expect("retry");
        let router = RouterPolicy::new(700_000, 300_000, 1).expect("router");
        let circuit = CircuitPolicy::new(1, 1_000).expect("circuit");
        let mut pool = ResilientProviderPool::new(retry, router, circuit);
        pool.add_provider(
            Arc::new(FixtureProvider::new(
                "cheap",
                "m1",
                [
                    Err(ProviderError::new(ProviderFailureClass::Transient, "one")),
                    Err(ProviderError::new(ProviderFailureClass::Transient, "two")),
                ],
            )),
            profile("cheap", "m1", 1_000_000, 800_000),
        )
        .expect("cheap provider");
        pool.add_provider(
            Arc::new(FixtureProvider::new(
                "strong",
                "m2",
                [Ok("done".to_owned())],
            )),
            profile("strong", "m2", 2_000_000, 950_000),
        )
        .expect("strong provider");

        let result = pool
            .complete(
                &provider_request(),
                &route_request(UserQualityMode::Economy),
                1_500,
                &NeverCancelled,
            )
            .expect("fallback succeeds");
        assert_eq!(result.endpoint_id, "strong");
        assert_eq!(result.failovers, 1);
        assert_eq!(result.attempts.len(), 2);
        assert!(matches!(
            result.attempts[0].outcome,
            AttemptOutcome::Failed(_)
        ));
        assert_eq!(result.attempts[1].outcome, AttemptOutcome::Success);
        assert_eq!(
            pool.profile("cheap").expect("profile").health.state,
            HealthState::OpenCircuit
        );
    }

    #[test]
    fn authentication_failure_is_not_retried_but_can_fail_over_to_distinct_profile() {
        let retry = RetryPolicy::new(3, 0, 0).expect("retry");
        let router = RouterPolicy::new(700_000, 300_000, 1).expect("router");
        let circuit = CircuitPolicy::new(2, 1_000).expect("circuit");
        let mut pool = ResilientProviderPool::new(retry, router, circuit);
        pool.add_provider(
            Arc::new(FixtureProvider::new(
                "cheap",
                "m1",
                [Err(ProviderError::new(
                    ProviderFailureClass::Authentication,
                    "expired",
                ))],
            )),
            profile("cheap", "m1", 1_000_000, 800_000),
        )
        .expect("cheap provider");
        pool.add_provider(
            Arc::new(FixtureProvider::new(
                "strong",
                "m2",
                [Ok("done".to_owned())],
            )),
            profile("strong", "m2", 2_000_000, 950_000),
        )
        .expect("strong provider");

        let result = pool
            .complete(
                &provider_request(),
                &route_request(UserQualityMode::Economy),
                1_500,
                &NeverCancelled,
            )
            .expect("alternate profile succeeds");
        assert_eq!(result.endpoint_id, "strong");
        assert_eq!(result.attempts[0].gateway_attempts, 1);
    }
}
