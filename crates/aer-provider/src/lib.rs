//! Provider-neutral request/response gateway with bounded retry and cancellation.
//!
//! This crate keeps provider-specific operational behavior behind a normalized
//! gateway and exposes the Step-11 routing/resilience control plane through the
//! `routing` module. Deterministic fixture adapters remain non-production and
//! allow core correctness tests to run without paid APIs or credentials.

pub mod delegated;
pub mod routing;

use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticationMethod {
    OAuthPkce,
    DeviceAuthorization,
    ApiKey,
    TestOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDescriptor {
    pub id: String,
    pub display_name: String,
    pub model: String,
    pub authentication: Vec<AuthenticationMethod>,
    pub production_ready: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRequest {
    pub run_id: String,
    pub attempt_id: String,
    pub instructions: String,
    pub input: String,
    pub response_schema: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProviderUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderResponse {
    pub provider_id: String,
    pub model: String,
    pub output_text: String,
    pub usage: ProviderUsage,
}

/// Provider-neutral operational failure taxonomy.
///
/// `Transient` and `Permanent` are retained as compatibility aliases for the
/// Phase-1 API. New adapters should prefer the more precise Step-11 variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderFailureClass {
    InvalidRequest,
    Authentication,
    Authorization,
    ContentPolicy,
    RateLimited,
    QuotaExhausted,
    TransientUnavailable,
    ProviderInternal,
    Timeout,
    Connection,
    StreamInterrupted,
    SchemaViolation,
    ContextOverflow,
    Cancelled,
    Unknown,
    Transient,
    Permanent,
}

impl ProviderFailureClass {
    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::RateLimited
                | Self::TransientUnavailable
                | Self::ProviderInternal
                | Self::Timeout
                | Self::Connection
                | Self::StreamInterrupted
                | Self::Transient
        )
    }

    #[must_use]
    pub const fn opens_circuit(self) -> bool {
        matches!(
            self,
            Self::TransientUnavailable
                | Self::ProviderInternal
                | Self::Timeout
                | Self::Connection
                | Self::StreamInterrupted
                | Self::Transient
        )
    }

    #[must_use]
    pub const fn fallback_eligible(self) -> bool {
        matches!(
            self,
            Self::Authentication
                | Self::Authorization
                | Self::RateLimited
                | Self::QuotaExhausted
                | Self::TransientUnavailable
                | Self::ProviderInternal
                | Self::Timeout
                | Self::Connection
                | Self::StreamInterrupted
                | Self::Transient
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderError {
    pub class: ProviderFailureClass,
    pub message: String,
    pub retry_after_ms: Option<u64>,
}

impl ProviderError {
    #[must_use]
    pub fn new(class: ProviderFailureClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: message.into(),
            retry_after_ms: None,
        }
    }

    #[must_use]
    pub fn with_retry_after_ms(mut self, retry_after_ms: u64) -> Self {
        self.retry_after_ms = Some(retry_after_ms);
        self
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.class.retryable()
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "provider {:?}: {}", self.class, self.message)
    }
}

impl Error for ProviderError {}

pub trait CancellationSignal: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

#[derive(Clone, Default)]
pub struct AtomicCancellation {
    cancelled: Arc<AtomicBool>,
}

impl AtomicCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl CancellationSignal for AtomicCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Default)]
pub struct NeverCancelled;

impl CancellationSignal for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

pub trait ProviderAdapter: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;

    fn complete(
        &self,
        request: &ProviderRequest,
        cancellation: &dyn CancellationSignal,
    ) -> Result<ProviderResponse, ProviderError>;
}

impl<T> ProviderAdapter for Arc<T>
where
    T: ProviderAdapter + ?Sized,
{
    fn descriptor(&self) -> ProviderDescriptor {
        (**self).descriptor()
    }

    fn complete(
        &self,
        request: &ProviderRequest,
        cancellation: &dyn CancellationSignal,
    ) -> Result<ProviderResponse, ProviderError> {
        (**self).complete(request, cancellation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl RetryPolicy {
    pub fn new(
        max_attempts: u32,
        base_backoff_ms: u64,
        max_backoff_ms: u64,
    ) -> Result<Self, GatewayError> {
        if max_attempts == 0 || max_backoff_ms < base_backoff_ms {
            return Err(GatewayError::InvalidRetryPolicy);
        }
        Ok(Self {
            max_attempts,
            base_backoff_ms,
            max_backoff_ms,
        })
    }

    fn delay_ms(self, completed_attempts: u32, error: &ProviderError) -> u64 {
        if let Some(provider_delay) = error.retry_after_ms {
            return provider_delay.min(self.max_backoff_ms);
        }
        if self.base_backoff_ms == 0 {
            return 0;
        }
        let exponent = completed_attempts.saturating_sub(1).min(20);
        self.base_backoff_ms
            .saturating_mul(1_u64 << exponent)
            .min(self.max_backoff_ms)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayResponse {
    pub response: ProviderResponse,
    pub attempts: u32,
}

pub struct ProviderGateway<P> {
    adapter: P,
    retry_policy: RetryPolicy,
}

impl<P> ProviderGateway<P>
where
    P: ProviderAdapter,
{
    #[must_use]
    pub const fn new(adapter: P, retry_policy: RetryPolicy) -> Self {
        Self {
            adapter,
            retry_policy,
        }
    }

    pub fn descriptor(&self) -> ProviderDescriptor {
        self.adapter.descriptor()
    }

    pub fn complete(
        &self,
        request: &ProviderRequest,
        cancellation: &dyn CancellationSignal,
    ) -> Result<GatewayResponse, GatewayError> {
        let mut attempts = 0_u32;
        loop {
            if cancellation.is_cancelled() {
                return Err(GatewayError::Provider(ProviderError::new(
                    ProviderFailureClass::Cancelled,
                    "provider request cancelled before attempt",
                )));
            }
            attempts = attempts
                .checked_add(1)
                .ok_or(GatewayError::AttemptOverflow)?;
            match self.adapter.complete(request, cancellation) {
                Ok(response) => return Ok(GatewayResponse { response, attempts }),
                Err(error) if error.retryable() && attempts < self.retry_policy.max_attempts => {
                    let delay_ms = self.retry_policy.delay_ms(attempts, &error);
                    wait_with_cancellation(delay_ms, cancellation)?;
                }
                Err(error) => return Err(GatewayError::Provider(error)),
            }
        }
    }
}

fn wait_with_cancellation(
    delay_ms: u64,
    cancellation: &dyn CancellationSignal,
) -> Result<(), GatewayError> {
    let mut remaining = delay_ms;
    while remaining > 0 {
        if cancellation.is_cancelled() {
            return Err(GatewayError::Provider(ProviderError::new(
                ProviderFailureClass::Cancelled,
                "provider retry cancelled during backoff",
            )));
        }
        let slice = remaining.min(25);
        thread::sleep(Duration::from_millis(slice));
        remaining -= slice;
    }
    if cancellation.is_cancelled() {
        return Err(GatewayError::Provider(ProviderError::new(
            ProviderFailureClass::Cancelled,
            "provider retry cancelled",
        )));
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub enum GatewayError {
    InvalidRetryPolicy,
    AttemptOverflow,
    Provider(ProviderError),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRetryPolicy => formatter.write_str(
                "provider retry policy requires max_attempts > 0 and max_backoff >= base_backoff",
            ),
            Self::AttemptOverflow => formatter.write_str("provider attempt counter overflow"),
            Self::Provider(error) => error.fmt(formatter),
        }
    }
}

impl Error for GatewayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provider(error) => Some(error),
            Self::InvalidRetryPolicy | Self::AttemptOverflow => None,
        }
    }
}

/// Deterministic, non-production provider used to exercise the real gateway and
/// runtime in CI without network access, credentials, or paid requests.
pub struct ReferenceProvider {
    model: String,
    script: Mutex<VecDeque<Result<String, ProviderError>>>,
}

impl ReferenceProvider {
    #[must_use]
    pub fn fixed(output: impl Into<String>) -> Self {
        Self::scripted([Ok(output.into())])
    }

    #[must_use]
    pub fn scripted<I>(steps: I) -> Self
    where
        I: IntoIterator<Item = Result<String, ProviderError>>,
    {
        Self {
            model: "reference-fixture-v1".to_owned(),
            script: Mutex::new(steps.into_iter().collect()),
        }
    }
}

impl ProviderAdapter for ReferenceProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: "reference".to_owned(),
            display_name: "Reference fixture provider".to_owned(),
            model: self.model.clone(),
            authentication: vec![AuthenticationMethod::TestOnly],
            production_ready: false,
        }
    }

    fn complete(
        &self,
        _request: &ProviderRequest,
        cancellation: &dyn CancellationSignal,
    ) -> Result<ProviderResponse, ProviderError> {
        if cancellation.is_cancelled() {
            return Err(ProviderError::new(
                ProviderFailureClass::Cancelled,
                "reference provider cancelled",
            ));
        }
        let step = self
            .script
            .lock()
            .expect("reference provider script mutex poisoned")
            .pop_front()
            .unwrap_or_else(|| {
                Err(ProviderError::new(
                    ProviderFailureClass::Permanent,
                    "reference provider script exhausted",
                ))
            })?;
        Ok(ProviderResponse {
            provider_id: "reference".to_owned(),
            model: self.model.clone(),
            output_text: step,
            usage: ProviderUsage::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AtomicCancellation, GatewayError, NeverCancelled, ProviderError, ProviderFailureClass,
        ProviderGateway, ProviderRequest, ReferenceProvider, RetryPolicy,
    };

    fn request() -> ProviderRequest {
        ProviderRequest {
            run_id: "run-1".to_owned(),
            attempt_id: "attempt-1".to_owned(),
            instructions: "return structured edits".to_owned(),
            input: "fix the fixture".to_owned(),
            response_schema: None,
        }
    }

    #[test]
    fn transient_failure_retries_with_a_hard_attempt_bound() {
        let provider = ReferenceProvider::scripted([
            Err(ProviderError::new(
                ProviderFailureClass::Transient,
                "temporary",
            )),
            Ok("done".to_owned()),
        ]);
        let gateway =
            ProviderGateway::new(provider, RetryPolicy::new(2, 0, 0).expect("retry policy"));
        let result = gateway
            .complete(&request(), &NeverCancelled)
            .expect("retry succeeds");
        assert_eq!(result.attempts, 2);
        assert_eq!(result.response.output_text, "done");
    }

    #[test]
    fn precise_transient_classes_are_retryable_but_invalid_and_auth_are_not() {
        assert!(ProviderFailureClass::Timeout.retryable());
        assert!(ProviderFailureClass::Connection.retryable());
        assert!(ProviderFailureClass::ProviderInternal.retryable());
        assert!(!ProviderFailureClass::InvalidRequest.retryable());
        assert!(!ProviderFailureClass::Authentication.retryable());
        assert!(!ProviderFailureClass::ContentPolicy.retryable());
    }

    #[test]
    fn authentication_failure_is_never_blindly_retried() {
        let provider = ReferenceProvider::scripted([Err(ProviderError::new(
            ProviderFailureClass::Authentication,
            "expired",
        ))]);
        let gateway =
            ProviderGateway::new(provider, RetryPolicy::new(4, 0, 0).expect("retry policy"));
        assert!(matches!(
            gateway.complete(&request(), &NeverCancelled),
            Err(GatewayError::Provider(ProviderError {
                class: ProviderFailureClass::Authentication,
                ..
            }))
        ));
    }

    #[test]
    fn cancellation_stops_before_provider_attempt() {
        let cancellation = AtomicCancellation::default();
        cancellation.cancel();
        let gateway = ProviderGateway::new(
            ReferenceProvider::fixed("unused"),
            RetryPolicy::new(2, 0, 0).expect("retry policy"),
        );
        assert!(matches!(
            gateway.complete(&request(), &cancellation),
            Err(GatewayError::Provider(ProviderError {
                class: ProviderFailureClass::Cancelled,
                ..
            }))
        ));
    }
}
