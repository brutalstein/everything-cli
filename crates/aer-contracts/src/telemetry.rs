//! Minimal OpenTelemetry-compatible contract-validation instrumentation.
//!
//! The API crate is used without an SDK/exporter. Applications may install their
//! own provider later; absent one, the global OpenTelemetry API remains a no-op.

use opentelemetry::{
    KeyValue, global,
    trace::{Span, Tracer},
};

/// Stage of executable-contract validation being observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationStage {
    MetaSchema,
    Compilation,
    Compatibility,
    Structural,
    Semantic,
}

impl ValidationStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MetaSchema => "meta_schema",
            Self::Compilation => "compilation",
            Self::Compatibility => "compatibility",
            Self::Structural => "structural",
            Self::Semantic => "semantic",
        }
    }
}

/// Privacy-safe telemetry payload for one validation stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractValidationObservation<'a> {
    pub contract: &'a str,
    pub schema_version: u32,
    pub stage: ValidationStage,
    pub valid: bool,
}

/// Narrow telemetry port owned by the contract subsystem.
pub trait ContractTelemetry: Send + Sync {
    fn record(&self, observation: ContractValidationObservation<'_>);
}

/// Explicit no-op implementation for tests and callers that disable telemetry.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopContractTelemetry;

impl ContractTelemetry for NoopContractTelemetry {
    fn record(&self, _observation: ContractValidationObservation<'_>) {}
}

/// Adapter to the process-global OpenTelemetry trace API.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenTelemetryContractTelemetry;

impl ContractTelemetry for OpenTelemetryContractTelemetry {
    fn record(&self, observation: ContractValidationObservation<'_>) {
        let tracer = global::tracer("aer-contracts");
        let mut span = tracer.start("aer.contract.validate");
        span.set_attribute(KeyValue::new(
            "aer.contract.name",
            observation.contract.to_owned(),
        ));
        span.set_attribute(KeyValue::new(
            "aer.contract.schema.version",
            i64::from(observation.schema_version),
        ));
        span.set_attribute(KeyValue::new(
            "aer.contract.validation.stage",
            observation.stage.as_str(),
        ));
        span.set_attribute(KeyValue::new(
            "aer.contract.validation.valid",
            observation.valid,
        ));
        span.end();
    }
}
