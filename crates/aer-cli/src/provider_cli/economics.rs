use std::{error::Error, io, path::Path};

use aer_core::model_context::ModelContextEnvelope;
use aer_provider::{
    NeverCancelled,
    delegated::{DelegatedCliProvider, DelegatedProviderKind, ModelIoTrace},
};

const BENCHMARK_VERSION: &str = "provider-context-economics-v1";
const CANONICAL_INPUT: &str = "Using only the supplied AER context, determine whether runtime permission mode may widen the capability ceiling. Reply exactly AER_CACHE_PROBE_OK if the answer is no; otherwise reply exactly AER_CACHE_PROBE_FAIL.";
const EXPECTED_OUTPUT: &str = "AER_CACHE_PROBE_OK";
const MIN_RUNS: u8 = 2;
const MAX_RUNS: u8 = 10;

pub(super) fn run(
    path: &Path,
    provider: &str,
    model: Option<String>,
    runs: u8,
    json: bool,
) -> Result<(), Box<dyn Error>> {
    validate_runs(runs)?;
    let provider = provider.parse::<DelegatedProviderKind>()?;
    if let Some(reason) = provider.delegated_smoke_block_reason() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} delegated benchmark is blocked fail-closed: {reason}",
                provider.display_name()
            ),
        )
        .into());
    }

    let context = ModelContextEnvelope::compile(path, CANONICAL_INPUT)?;
    let adapter = DelegatedCliProvider::new(
        provider,
        context.rendered.clone(),
        context.digest.clone(),
        model,
    );

    if !json {
        println!("everything provider context economics");
        println!("  benchmark  {BENCHMARK_VERSION}");
        println!("  provider   {}", provider.display_name());
        println!("  transport  {}", provider.transport());
        println!("  runs       {runs}");
        println!("  context    {}", short_id(&context.digest));
        println!("  core       {}", short_id(&context.architecture.digest));
        println!("  pack       {}", short_id(&context.task_context.pack_id));
        println!(
            "  selected   {} estimated token units · {} items",
            context.task_context.total_token_cost(),
            context.task_context.items.len()
        );
        println!("\ncalling model…");
    }

    let mut samples = Vec::with_capacity(usize::from(runs));
    for run in 1..=runs {
        let trace = adapter.smoke(CANONICAL_INPUT, &NeverCancelled)?;
        let sample = Sample::from_trace(run, &trace);
        if !json {
            println!(
                "  run {run:<2} fresh {} · write {} · read {} · read-share {} · out {} · {} ms · contract {}",
                display_u64(sample.fresh_input_tokens),
                display_u64(sample.cache_creation_input_tokens),
                display_u64(sample.cache_read_input_tokens),
                display_bps(sample.cache_read_share_bps),
                display_u64(sample.output_tokens),
                sample.duration_ms,
                if sample.output_contract_pass {
                    "PASS"
                } else {
                    "FAIL"
                },
            );
        }
        samples.push(sample);
    }

    let report = Report::from_samples(samples);
    if json {
        print_json(provider, runs, &context, &report)?;
    } else {
        print_report(&report);
    }
    Ok(())
}

fn print_json(
    provider: DelegatedProviderKind,
    runs: u8,
    context: &ModelContextEnvelope,
    report: &Report,
) -> Result<(), serde_json::Error> {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "benchmark_version": report.version,
            "provider": provider.id(),
            "transport": provider.transport(),
            "runs": runs,
            "canonical_input": CANONICAL_INPUT,
            "expected_output": EXPECTED_OUTPUT,
            "model_context_digest": context.digest,
            "model_context_estimated_tokens": context.estimated_tokens,
            "architecture_core": {
                "digest": context.architecture.digest,
                "policy_version": context.architecture.policy_version,
                "estimated_tokens": context.architecture.estimated_tokens,
            },
            "context_pack": {
                "pack_id": context.task_context.pack_id,
                "repo_snapshot": context.task_context.repo_snapshot,
                "selected_token_cost": context.task_context.total_token_cost(),
                "items": context.task_context.items.iter().map(|item| serde_json::json!({
                    "path": item.path,
                    "source_ref": item.source_ref,
                    "content_hash": item.content_hash,
                    "token_cost": item.token_cost,
                    "tier": item.tier.as_u8(),
                })).collect::<Vec<_>>(),
            },
            "measurement": {
                "valid": report.measurement_valid,
                "output_contract_pass": report.output_contract_pass,
                "model_context_digest_stable": report.model_context_digest_stable,
                "resolved_models_stable": report.resolved_models_stable,
                "exact_input_complete": report.exact_input_complete,
                "exact_input_min": report.exact_input_min,
                "exact_input_max": report.exact_input_max,
                "exact_input_spread_tokens": report.exact_input_spread_tokens,
                "exact_input_median": report.exact_input_median,
                "steady_state_fresh_input_median": report.steady_state_fresh_input_median,
                "steady_state_cache_creation_median": report.steady_state_cache_creation_median,
                "steady_state_cache_read_median": report.steady_state_cache_read_median,
                "steady_state_fresh_share_bps_median": report.steady_state_fresh_share_bps_median,
                "steady_state_cache_creation_share_bps_median": report.steady_state_cache_creation_share_bps_median,
                "steady_state_cache_read_share_bps_median": report.steady_state_cache_read_share_bps_median,
                "first_to_steady_cache_read_delta_tokens": report.first_to_steady_cache_read_delta_tokens,
                "first_to_steady_cache_creation_delta_tokens": report.first_to_steady_cache_creation_delta_tokens,
            },
            "samples": report.samples.iter().map(|sample| serde_json::json!({
                "run": sample.run,
                "model_context_digest": sample.model_context_digest,
                "resolved_models": sample.resolved_models,
                "provider_request_id": sample.provider_request_id,
                "fresh_input_tokens": sample.fresh_input_tokens,
                "cache_creation_input_tokens": sample.cache_creation_input_tokens,
                "cache_read_input_tokens": sample.cache_read_input_tokens,
                "exact_observed_input_tokens": sample.exact_observed_input_tokens,
                "fresh_input_share_bps": sample.fresh_input_share_bps,
                "cache_creation_share_bps": sample.cache_creation_share_bps,
                "cache_read_share_bps": sample.cache_read_share_bps,
                "output_tokens": sample.output_tokens,
                "reasoning_output_tokens": sample.reasoning_output_tokens,
                "provider_cost_usd": sample.provider_cost_usd,
                "duration_ms": sample.duration_ms,
                "output_contract_pass": sample.output_contract_pass,
            })).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}

fn print_report(report: &Report) {
    println!("\nmeasurement");
    println!(
        "  integrity  {}",
        if report.measurement_valid {
            "PASS"
        } else {
            "FAIL"
        }
    );
    println!(
        "  context    {}",
        if report.model_context_digest_stable {
            "stable"
        } else {
            "DRIFT"
        }
    );
    println!(
        "  models     {}",
        if report.resolved_models_stable {
            "stable"
        } else {
            "DRIFT"
        }
    );
    println!(
        "  input      median {} · range {}..{} · spread {}",
        display_u64(report.exact_input_median),
        display_u64(report.exact_input_min),
        display_u64(report.exact_input_max),
        display_u64(report.exact_input_spread_tokens),
    );
    println!(
        "  steady     fresh {} ({}) · write {} ({}) · read {} ({})",
        display_u64(report.steady_state_fresh_input_median),
        display_bps(report.steady_state_fresh_share_bps_median),
        display_u64(report.steady_state_cache_creation_median),
        display_bps(report.steady_state_cache_creation_share_bps_median),
        display_u64(report.steady_state_cache_read_median),
        display_bps(report.steady_state_cache_read_share_bps_median),
    );
    println!(
        "  delta      cache-read {} · cache-write {} tokens vs first run",
        display_i128(report.first_to_steady_cache_read_delta_tokens),
        display_i128(report.first_to_steady_cache_creation_delta_tokens),
    );
    println!(
        "  note       input cache economics are separate from output tokens and provider-reported total cost"
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Sample {
    run: u8,
    model_context_digest: String,
    resolved_models: Vec<String>,
    provider_request_id: Option<String>,
    fresh_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    exact_observed_input_tokens: Option<u64>,
    fresh_input_share_bps: Option<u32>,
    cache_creation_share_bps: Option<u32>,
    cache_read_share_bps: Option<u32>,
    output_tokens: Option<u64>,
    reasoning_output_tokens: Option<u64>,
    provider_cost_usd: Option<String>,
    duration_ms: u128,
    output_contract_pass: bool,
}

impl Sample {
    fn from_trace(run: u8, trace: &ModelIoTrace) -> Self {
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
            fresh_input_share_bps: ratio_basis_points(
                trace.usage.input_tokens,
                exact_observed_input_tokens,
            ),
            cache_creation_share_bps: ratio_basis_points(
                trace.usage.cache_creation_input_tokens,
                exact_observed_input_tokens,
            ),
            cache_read_share_bps: ratio_basis_points(
                trace.usage.cache_read_input_tokens,
                exact_observed_input_tokens,
            ),
            output_tokens: trace.usage.output_tokens,
            reasoning_output_tokens: trace.usage.reasoning_output_tokens,
            provider_cost_usd: trace.provider_cost_usd.clone(),
            duration_ms: trace.duration_ms,
            output_contract_pass: trace.output.trim() == EXPECTED_OUTPUT,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Report {
    version: &'static str,
    samples: Vec<Sample>,
    measurement_valid: bool,
    output_contract_pass: bool,
    model_context_digest_stable: bool,
    resolved_models_stable: bool,
    exact_input_complete: bool,
    exact_input_min: Option<u64>,
    exact_input_max: Option<u64>,
    exact_input_spread_tokens: Option<u64>,
    exact_input_median: Option<u64>,
    steady_state_fresh_input_median: Option<u64>,
    steady_state_cache_creation_median: Option<u64>,
    steady_state_cache_read_median: Option<u64>,
    steady_state_fresh_share_bps_median: Option<u32>,
    steady_state_cache_creation_share_bps_median: Option<u32>,
    steady_state_cache_read_share_bps_median: Option<u32>,
    first_to_steady_cache_read_delta_tokens: Option<i128>,
    first_to_steady_cache_creation_delta_tokens: Option<i128>,
}

impl Report {
    fn from_samples(samples: Vec<Sample>) -> Self {
        let output_contract_pass =
            !samples.is_empty() && samples.iter().all(|sample| sample.output_contract_pass);
        let model_context_digest_stable = samples.len() >= 2
            && samples
                .windows(2)
                .all(|pair| pair[0].model_context_digest == pair[1].model_context_digest);
        let resolved_models_stable = samples.len() >= 2
            && samples
                .windows(2)
                .all(|pair| pair[0].resolved_models == pair[1].resolved_models);
        let exact_input_complete = samples
            .iter()
            .all(|sample| sample.exact_observed_input_tokens.is_some());

        let exact_inputs =
            collect_complete_u64(&samples, |sample| sample.exact_observed_input_tokens);
        let exact_input_min = exact_inputs
            .as_ref()
            .and_then(|values| values.iter().min().copied());
        let exact_input_max = exact_inputs
            .as_ref()
            .and_then(|values| values.iter().max().copied());
        let exact_input_spread_tokens = exact_input_min
            .zip(exact_input_max)
            .and_then(|(minimum, maximum)| maximum.checked_sub(minimum));
        let exact_input_median = exact_inputs.as_ref().and_then(|values| median_u64(values));

        let steady = samples.get(1..).unwrap_or(&[]);
        let steady_state_fresh_input_median =
            median_complete_u64(steady, |sample| sample.fresh_input_tokens);
        let steady_state_cache_creation_median =
            median_complete_u64(steady, |sample| sample.cache_creation_input_tokens);
        let steady_state_cache_read_median =
            median_complete_u64(steady, |sample| sample.cache_read_input_tokens);
        let steady_state_fresh_share_bps_median =
            median_complete_u32(steady, |sample| sample.fresh_input_share_bps);
        let steady_state_cache_creation_share_bps_median =
            median_complete_u32(steady, |sample| sample.cache_creation_share_bps);
        let steady_state_cache_read_share_bps_median =
            median_complete_u32(steady, |sample| sample.cache_read_share_bps);

        let first_to_steady_cache_read_delta_tokens = samples
            .first()
            .and_then(|sample| sample.cache_read_input_tokens)
            .zip(steady_state_cache_read_median)
            .map(|(first, steady)| i128::from(steady) - i128::from(first));
        let first_to_steady_cache_creation_delta_tokens = samples
            .first()
            .and_then(|sample| sample.cache_creation_input_tokens)
            .zip(steady_state_cache_creation_median)
            .map(|(first, steady)| i128::from(steady) - i128::from(first));

        let measurement_valid = samples.len() >= 2
            && output_contract_pass
            && model_context_digest_stable
            && resolved_models_stable
            && exact_input_complete;

        Self {
            version: BENCHMARK_VERSION,
            samples,
            measurement_valid,
            output_contract_pass,
            model_context_digest_stable,
            resolved_models_stable,
            exact_input_complete,
            exact_input_min,
            exact_input_max,
            exact_input_spread_tokens,
            exact_input_median,
            steady_state_fresh_input_median,
            steady_state_cache_creation_median,
            steady_state_cache_read_median,
            steady_state_fresh_share_bps_median,
            steady_state_cache_creation_share_bps_median,
            steady_state_cache_read_share_bps_median,
            first_to_steady_cache_read_delta_tokens,
            first_to_steady_cache_creation_delta_tokens,
        }
    }
}

fn validate_runs(runs: u8) -> Result<(), io::Error> {
    if (MIN_RUNS..=MAX_RUNS).contains(&runs) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("provider benchmark runs must be between {MIN_RUNS} and {MAX_RUNS}"),
    ))
}

fn collect_complete_u64<F>(samples: &[Sample], projection: F) -> Option<Vec<u64>>
where
    F: FnMut(&Sample) -> Option<u64>,
{
    samples.iter().map(projection).collect()
}

fn median_complete_u64<F>(samples: &[Sample], projection: F) -> Option<u64>
where
    F: FnMut(&Sample) -> Option<u64>,
{
    collect_complete_u64(samples, projection)
        .as_ref()
        .and_then(|values| median_u64(values))
}

fn median_complete_u32<F>(samples: &[Sample], projection: F) -> Option<u32>
where
    F: FnMut(&Sample) -> Option<u32>,
{
    let values = samples.iter().map(projection).collect::<Option<Vec<_>>>()?;
    median_u32(&values)
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
    let values = values
        .iter()
        .map(|value| u64::from(*value))
        .collect::<Vec<_>>();
    median_u64(&values).and_then(|value| u32::try_from(value).ok())
}

fn short_id(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

fn display_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
}

fn display_i128(value: Option<i128>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
}

fn display_bps(value: Option<u32>) -> String {
    value.map_or_else(
        || "unknown".to_owned(),
        |value| format!("{}.{:02}%", value / 100, value % 100),
    )
}

#[cfg(test)]
mod tests {
    use aer_provider::ProviderUsage;

    use super::*;

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
    fn run_count_is_bounded() {
        assert!(validate_runs(2).is_ok());
        assert!(validate_runs(10).is_ok());
        assert!(validate_runs(1).is_err());
        assert!(validate_runs(11).is_err());
    }

    #[test]
    fn report_separates_output_variance_from_input_cache_profile() {
        let first = Sample::from_trace(
            1,
            &trace(
                "same",
                Some(2),
                Some(6_800),
                Some(4_200),
                1,
                EXPECTED_OUTPUT,
            ),
        );
        let second = Sample::from_trace(
            2,
            &trace(
                "same",
                Some(2),
                Some(1_800),
                Some(9_200),
                4_000,
                EXPECTED_OUTPUT,
            ),
        );
        let third = Sample::from_trace(
            3,
            &trace(
                "same",
                Some(2),
                Some(2_000),
                Some(9_000),
                7,
                EXPECTED_OUTPUT,
            ),
        );

        let report = Report::from_samples(vec![first, second, third]);
        assert!(report.measurement_valid);
        assert_eq!(report.steady_state_cache_read_median, Some(9_100));
        assert_eq!(report.steady_state_cache_creation_median, Some(1_900));
        assert_eq!(report.first_to_steady_cache_read_delta_tokens, Some(4_900));
        assert_eq!(
            report.first_to_steady_cache_creation_delta_tokens,
            Some(-4_900)
        );
    }

    #[test]
    fn report_fails_closed_on_context_drift_or_missing_usage() {
        let first = Sample::from_trace(
            1,
            &trace("a", Some(2), Some(6_800), Some(4_200), 1, EXPECTED_OUTPUT),
        );
        let second = Sample::from_trace(
            2,
            &trace("b", None, Some(6_800), Some(4_200), 1, EXPECTED_OUTPUT),
        );
        let report = Report::from_samples(vec![first, second]);
        assert!(!report.model_context_digest_stable);
        assert!(!report.exact_input_complete);
        assert!(!report.measurement_valid);
    }
}
