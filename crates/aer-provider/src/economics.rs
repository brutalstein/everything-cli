//! Provider-context economics measurement without provider-specific price guesses.
//!
//! The benchmark deliberately separates input-cache behavior from output variance.
//! Provider-reported token dimensions are preserved exactly; missing dimensions
//! stay unknown rather than being synthesized.

use crate::delegated::ModelIoTrace;

pub const PROVIDER_CONTEXT_ECONOMICS_VERSION: &str = "provider-context-economics-v1";
pub const CANONICAL_BENCHMARK_INPUT: &str = "Using only the supplied AER context, determine whether runtime permission mode may widen the capability ceiling. Reply exactly AER_CACHE_PROBE_OK if the answer is no; otherwise reply exactly AER_CACHE_PROBE_FAIL.";
pub const CANONICAL_EXPECTED_OUTPUT: &str = "AER_CACHE_PROBE_OK";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEconomicsSample {
    pub run: u8,
    pub model_context_digest: String,
    pub resolved_models: Vec<String>,
    pub provider_request_id: Option<String>,
    pub fresh_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub exact_observed_input_tokens: Option<u64>,
    pub cache_read_share_bps: Option<u32>,
    pub cache_creation_share_bps: Option<u32>,
    pub fresh_input_share_bps: Option<u32>,
    pub output_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub provider_cost_usd: Option<String>,
    pub duration_ms: u128,
    pub output_contract_pass: bool,
}

impl ProviderEconomicsSample {
    #[must_use]
    pub fn from_trace(run: u8, trace: &ModelIoTrace, expected_output: &str) -> Self {
        let exact_observed_input_tokens = trace.usage.exact_observed_input_tokens();
        Self {
            run,
            model_context_digest: trace.architecture_context_digest.clone(),
            resolved_models: trace.resolved_models.clone(),
            provider_request_id: trace.provider_request_id.clone(),
            fresh_input_tokens: trace.usage.input_tokens,
            cache_creation_input_tokens: trace.usage.cache_creation_input_tokens,
            cache_read_input_tokens: trace.usage.cache_read_input_tokens,
            exact_observed_input_tokens,
            cache_read_share_bps: ratio_basis_points(
                trace.usage.cache_read_input_tokens,
                exact_observed_input_tokens,
            ),
            cache_creation_share_bps: ratio_basis_points(
                trace.usage.cache_creation_input_tokens,
                exact_observed_input_tokens,
            ),
            fresh_input_share_bps: ratio_basis_points(
                trace.usage.input_tokens,
                exact_observed_input_tokens,
            ),
            output_tokens: trace.usage.output_tokens,
            reasoning_output_tokens: trace.usage.reasoning_output_tokens,
            provider_cost_usd: trace.provider_cost_usd.clone(),
            duration_ms: trace.duration_ms,
            output_contract_pass: trace.output.trim() == expected_output,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderEconomicsReport {
    pub version: &'static str,
    pub samples: Vec<ProviderEconomicsSample>,
    pub measurement_valid: bool,
    pub output_contract_pass: bool,
    pub model_context_digest_stable: bool,
    pub resolved_models_stable: bool,
    pub exact_input_complete: bool,
    pub exact_input_min: Option<u64>,
    pub exact_input_max: Option<u64>,
    pub exact_input_median: Option<u64>,
    pub steady_state_fresh_input_median: Option<u64>,
    pub steady_state_cache_creation_median: Option<u64>,
    pub steady_state_cache_read_median: Option<u64>,
    pub steady_state_cache_read_share_bps_median: Option<u32>,
    pub steady_state_cache_creation_share_bps_median: Option<u32>,
    pub first_to_steady_cache_read_delta_tokens: Option<i128>,
    pub first_to_steady_cache_creation_delta_tokens: Option<i128>,
}

impl ProviderEconomicsReport {
    #[must_use]
    pub fn from_samples(samples: Vec<ProviderEconomicsSample>) -> Self {
        let output_contract_pass = !samples.is_empty()
            && samples.iter().all(|sample| sample.output_contract_pass);
        let model_context_digest_stable = same_by(&samples, |sample| {
            sample.model_context_digest.as_str()
        });
        let resolved_models_stable = same_by(&samples, |sample| sample.resolved_models.as_slice());
        let exact_input_complete = samples
            .iter()
            .all(|sample| sample.exact_observed_input_tokens.is_some());

        let exact_inputs = collect_complete(&samples, |sample| sample.exact_observed_input_tokens);
        let exact_input_min = exact_inputs.as_ref().and_then(|values| values.iter().min().copied());
        let exact_input_max = exact_inputs.as_ref().and_then(|values| values.iter().max().copied());
        let exact_input_median = exact_inputs.as_ref().and_then(|values| median_u64(values));

        let steady = samples.get(1..).unwrap_or(&[]);
        let steady_fresh = collect_complete(steady, |sample| sample.fresh_input_tokens)
            .as_ref()
            .and_then(|values| median_u64(values));
        let steady_cache_creation = collect_complete(steady, |sample| {
            sample.cache_creation_input_tokens
        })
        .as_ref()
        .and_then(|values| median_u64(values));
        let steady_cache_read = collect_complete(steady, |sample| sample.cache_read_input_tokens)
            .as_ref()
            .and_then(|values| median_u64(values));
        let steady_cache_read_share = collect_complete_u32(steady, |sample| {
            sample.cache_read_share_bps
        })
        .as_ref()
        .and_then(|values| median_u32(values));
        let steady_cache_creation_share = collect_complete_u32(steady, |sample| {
            sample.cache_creation_share_bps
        })
        .as_ref()
        .and_then(|values| median_u32(values));

        let first_to_steady_cache_read_delta_tokens = samples
            .first()
            .and_then(|sample| sample.cache_read_input_tokens)
            .zip(steady_cache_read)
            .map(|(first, steady)| i128::from(steady) - i128::from(first));
        let first_to_steady_cache_creation_delta_tokens = samples
            .first()
            .and_then(|sample| sample.cache_creation_input_tokens)
            .zip(steady_cache_creation)
            .map(|(first, steady)| i128::from(steady) - i128::from(first));

        let measurement_valid = samples.len() >= 2
            && output_contract_pass
            && model_context_digest_stable
            && resolved_models_stable
            && exact_input_complete;

        Self {
            version: PROVIDER_CONTEXT_ECONOMICS_VERSION,
            samples,
            measurement_valid,
            output_contract_pass,
            model_context_digest_stable,
            resolved_models_stable,
            exact_input_complete,
            exact_input_min,
            exact_input_max,
            exact_input_median,
            steady_state_fresh_input_median: steady_fresh,
            steady_state_cache_creation_median: steady_cache_creation,
            steady_state_cache_read_median: steady_cache_read,
            steady_state_cache_read_share_bps_median: steady_cache_read_share,
            steady_state_cache_creation_share_bps_median: steady_cache_creation_share,
            first_to_steady_cache_read_delta_tokens,
            first_to_steady_cache_creation_delta_tokens,
        }
    }
}

fn same_by<T, F>(samples: &[ProviderEconomicsSample], mut projection: F) -> bool
where
    T: PartialEq + ?Sized,
    F: FnMut(&ProviderEconomicsSample) -> &T,
{
    let Some(first) = samples.first() else {
        return false;
    };
    let first = projection(first);
    samples.iter().skip(1).all(|sample| projection(sample) == first)
}

fn collect_complete<F>(samples: &[ProviderEconomicsSample], mut projection: F) -> Option<Vec<u64>>
where
    F: FnMut(&ProviderEconomicsSample) -> Option<u64>,
{
    samples.iter().map(&mut projection).collect()
}

fn collect_complete_u32<F>(
    samples: &[ProviderEconomicsSample],
    mut projection: F,
) -> Option<Vec<u32>>
where
    F: FnMut(&ProviderEconomicsSample) -> Option<u32>,
{
    samples.iter().map(&mut projection).collect()
}

fn ratio_basis_points(part: Option<u64>, total: Option<u64>) -> Option<u32> {
    let part = u128::from(part?);
    let total = u128::from(total?);
    if total == 0 {
        return None;
    }
    let basis_points = part.checked_mul(10_000)?.checked_div(total)?;
    u32::try_from(basis_points).ok()
}

fn median_u64(values: &[u64]) -> Option<u64> {
    let mut values = values.to_vec();
    values.sort_unstable();
    median_sorted_u64(&values)
}

fn median_sorted_u64(values: &[u64]) -> Option<u64> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    if len % 2 == 1 {
        return values.get(len / 2).copied();
    }
    let left = u128::from(*values.get(len / 2 - 1)?);
    let right = u128::from(*values.get(len / 2)?);
    u64::try_from((left + right) / 2).ok()
}

fn median_u32(values: &[u32]) -> Option<u32> {
    let values = values.iter().map(|value| u64::from(*value)).collect::<Vec<_>>();
    median_u64(&values).and_then(|value| u32::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use crate::{ProviderUsage, delegated::ModelIoTrace};

    use super::{
        CANONICAL_EXPECTED_OUTPUT, ProviderEconomicsReport, ProviderEconomicsSample,
    };

    fn trace(
        digest: &str,
        fresh: Option<u64>,
        created: Option<u64>,
        read: Option<u64>,
        output_tokens: u64,
        output: &str,
    ) -> ModelIoTrace {
        ModelIoTrace {
            provider: "claude".to_owned(),
            transport: "claude-print-json".to_owned(),
            requested_model: None,
            resolved_models: vec!["claude-test".to_owned()],
            provider_cost_usd: Some("0.01".to_owned()),
            provider_request_id: Some("request".to_owned()),
            architecture_context_digest: digest.to_owned(),
            input: "probe".to_owned(),
            output: output.to_owned(),
            usage: ProviderUsage {
                input_tokens: fresh,
                cache_creation_input_tokens: created,
                cache_read_input_tokens: read,
                output_tokens: Some(output_tokens),
                reasoning_output_tokens: Some(0),
            },
            duration_ms: 100,
            raw_event_count: 1,
        }
    }

    #[test]
    fn sample_preserves_exact_cache_dimensions_and_shares() {
        let trace = trace(
            "digest",
            Some(100),
            Some(600),
            Some(300),
            1,
            CANONICAL_EXPECTED_OUTPUT,
        );
        let sample = ProviderEconomicsSample::from_trace(1, &trace, CANONICAL_EXPECTED_OUTPUT);
        assert_eq!(sample.exact_observed_input_tokens, Some(1_000));
        assert_eq!(sample.fresh_input_share_bps, Some(1_000));
        assert_eq!(sample.cache_creation_share_bps, Some(6_000));
        assert_eq!(sample.cache_read_share_bps, Some(3_000));
        assert!(sample.output_contract_pass);
    }

    #[test]
    fn report_keeps_output_variance_out_of_input_cache_profile() {
        let first = ProviderEconomicsSample::from_trace(
            1,
            &trace(
                "same",
                Some(2),
                Some(6_800),
                Some(4_200),
                1,
                CANONICAL_EXPECTED_OUTPUT,
            ),
            CANONICAL_EXPECTED_OUTPUT,
        );
        let second = ProviderEconomicsSample::from_trace(
            2,
            &trace(
                "same",
                Some(2),
                Some(1_800),
                Some(9_200),
                4_000,
                CANONICAL_EXPECTED_OUTPUT,
            ),
            CANONICAL_EXPECTED_OUTPUT,
        );
        let third = ProviderEconomicsSample::from_trace(
            3,
            &trace(
                "same",
                Some(2),
                Some(2_000),
                Some(9_000),
                7,
                CANONICAL_EXPECTED_OUTPUT,
            ),
            CANONICAL_EXPECTED_OUTPUT,
        );

        let report = ProviderEconomicsReport::from_samples(vec![first, second, third]);
        assert!(report.measurement_valid);
        assert_eq!(report.steady_state_cache_read_median, Some(9_100));
        assert_eq!(report.steady_state_cache_creation_median, Some(1_900));
        assert_eq!(report.first_to_steady_cache_read_delta_tokens, Some(4_900));
        assert_eq!(report.first_to_steady_cache_creation_delta_tokens, Some(-4_900));
    }

    #[test]
    fn report_fails_measurement_integrity_on_context_drift_or_missing_usage() {
        let first = ProviderEconomicsSample::from_trace(
            1,
            &trace(
                "a",
                Some(2),
                Some(6_800),
                Some(4_200),
                1,
                CANONICAL_EXPECTED_OUTPUT,
            ),
            CANONICAL_EXPECTED_OUTPUT,
        );
        let second = ProviderEconomicsSample::from_trace(
            2,
            &trace(
                "b",
                None,
                Some(6_800),
                Some(4_200),
                1,
                CANONICAL_EXPECTED_OUTPUT,
            ),
            CANONICAL_EXPECTED_OUTPUT,
        );
        let report = ProviderEconomicsReport::from_samples(vec![first, second]);
        assert!(!report.model_context_digest_stable);
        assert!(!report.exact_input_complete);
        assert!(!report.measurement_valid);
    }
}
