use std::{
    env,
    error::Error,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use aer_core::model_context::ModelContextEnvelope;
use aer_provider::{
    NeverCancelled, ProviderUsage,
    delegated::{DelegatedCliProvider, DelegatedProviderKind, LoginFlow, ModelIoTrace},
};
use clap::{Parser, Subcommand};

const GEMINI_DELEGATED_ISOLATION_BLOCK: &str = "current Gemini CLI delegated OAuth keeps authentication and user behavior/configuration under the same user state; its home .gemini/.env fallback can still inject process configuration even with --ignore-env. AER will not copy OAuth credentials or claim isolation it cannot enforce";
const PROVIDER_CONTEXT_ECONOMICS_VERSION: &str = "provider-context-economics-v1";
const PROVIDER_CONTEXT_ECONOMICS_INPUT: &str = "Using only the supplied AER context, determine whether runtime permission mode may widen the capability ceiling. Reply exactly AER_CACHE_PROBE_OK if the answer is no; otherwise reply exactly AER_CACHE_PROBE_FAIL.";
const PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT: &str = "AER_CACHE_PROBE_OK";
const MIN_PROVIDER_BENCHMARK_RUNS: u8 = 2;
const MAX_PROVIDER_BENCHMARK_RUNS: u8 = 10;

#[derive(Parser, Debug)]
#[command(name = "everything")]
struct ProviderSurface {
    #[arg(long, global = true, value_name = "PATH")]
    workspace: Option<PathBuf>,

    #[command(subcommand)]
    command: ProviderTopLevel,
}

#[derive(Subcommand, Debug)]
enum ProviderTopLevel {
    /// Inspect all supported local provider transports.
    Providers {
        #[arg(long)]
        json: bool,
    },
    /// Connect, inspect or exercise one model provider.
    Provider {
        #[command(subcommand)]
        command: ProviderCommand,
    },
}

#[derive(Subcommand, Debug)]
enum ProviderCommand {
    /// Show provider installation and delegated-auth state.
    Status {
        provider: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Start the vendor-owned OAuth/authentication flow.
    Login {
        provider: String,
        /// Use a device-code flow when the vendor supports it (currently Codex).
        #[arg(long)]
        device: bool,
    },
    /// Clear the vendor-owned local authentication session when supported.
    Logout { provider: String },
    /// Make one real, read-only model call with bounded constitutional + RI2 context.
    Smoke {
        provider: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(
            long,
            default_value = "State the product name and one architecture rule you were given."
        )]
        prompt: String,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        show_input: bool,
    },
    /// Measure repeated provider input-cache economics with a canonical bounded probe.
    Benchmark {
        provider: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value_t = 3)]
        runs: u8,
        #[arg(long)]
        json: bool,
    },
}

/// Intercepts only provider-specific commands so the ordinary CLI path remains
/// lazy and provider discovery never becomes startup work.
pub(crate) fn try_run_provider_surface() -> Result<bool, Box<dyn Error>> {
    let args = env::args_os().collect::<Vec<_>>();
    if !contains_provider_command(&args) {
        return Ok(false);
    }
    let cli = ProviderSurface::try_parse_from(args)?;
    let cwd = env::current_dir()?;
    let workspace = cli.workspace.map_or(cwd.clone(), |path| {
        if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
    });

    match cli.command {
        ProviderTopLevel::Providers { json } => print_providers(&workspace, json)?,
        ProviderTopLevel::Provider { command } => match command {
            ProviderCommand::Status { provider, json } => match provider {
                Some(provider) => provider_status(&workspace, &provider, json)?,
                None => print_providers(&workspace, json)?,
            },
            ProviderCommand::Login { provider, device } => {
                provider_login(&workspace, &provider, device)?;
            }
            ProviderCommand::Logout { provider } => provider_logout(&workspace, &provider)?,
            ProviderCommand::Smoke {
                provider,
                model,
                prompt,
                json,
                show_input,
            } => provider_smoke(&workspace, &provider, model, &prompt, json, show_input)?,
            ProviderCommand::Benchmark {
                provider,
                model,
                runs,
                json,
            } => provider_benchmark(&workspace, &provider, model, runs, json)?,
        },
    }
    Ok(true)
}

fn contains_provider_command(args: &[OsString]) -> bool {
    let mut arguments = args.iter().skip(1);
    while let Some(argument) = arguments.next() {
        let value = argument.to_string_lossy();
        if value == "--workspace" {
            let _ = arguments.next();
            continue;
        }
        if value.starts_with("--workspace=") || value.starts_with('-') {
            continue;
        }
        return value == "provider" || value == "providers";
    }
    false
}

fn provider_smoke_block_reason(provider: DelegatedProviderKind) -> Option<&'static str> {
    match provider {
        DelegatedProviderKind::Gemini => Some(GEMINI_DELEGATED_ISOLATION_BLOCK),
        DelegatedProviderKind::Codex | DelegatedProviderKind::Claude => None,
    }
}

fn ensure_provider_smoke_eligible(provider: DelegatedProviderKind) -> Result<(), io::Error> {
    if let Some(reason) = provider_smoke_block_reason(provider) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} delegated smoke is blocked fail-closed: {reason}",
                provider.display_name()
            ),
        ));
    }
    Ok(())
}

pub(crate) fn print_providers(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let statuses = DelegatedProviderKind::ALL
        .into_iter()
        .map(|provider| DelegatedCliProvider::status(provider, path))
        .collect::<Vec<_>>();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &statuses
                    .iter()
                    .map(|status| serde_json::json!({
                        "provider": status.provider.id(),
                        "display_name": status.provider.display_name(),
                        "installed": status.installed,
                        "version": status.version,
                        "authentication": status.authentication.as_str(),
                        "authentication_method": status.authentication_method,
                        "account_plan": status.account_plan,
                        "smoke_eligible": provider_smoke_block_reason(status.provider).is_none(),
                        "smoke_block_reason": provider_smoke_block_reason(status.provider),
                        "detail": status.detail,
                    }))
                    .collect::<Vec<_>>()
            )?
        );
        return Ok(());
    }

    println!("everything providers");
    for status in statuses {
        println!(
            "  {:<7} {:<15} {}{}",
            status.provider.id(),
            status.authentication.as_str(),
            status.version.as_deref().unwrap_or("not installed"),
            status
                .account_plan
                .as_deref()
                .map(|plan| format!(" · {plan}"))
                .unwrap_or_default()
        );
        if status.installed && !status.detail.trim().is_empty() {
            println!("           {}", status.detail);
        }
        if let Some(reason) = provider_smoke_block_reason(status.provider) {
            println!("           smoke blocked · {reason}");
        }
    }
    println!("\nconnect   everything provider login <codex|claude|gemini>");
    println!("verify    everything provider smoke <provider> --show-input --prompt <text>");
    println!("benchmark everything provider benchmark <provider> --runs 3");
    Ok(())
}

pub(crate) fn provider_status(
    path: &Path,
    provider: &str,
    json: bool,
) -> Result<(), Box<dyn Error>> {
    let provider = parse_provider(provider)?;
    let status = DelegatedCliProvider::status(provider, path);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "provider": status.provider.id(),
                "display_name": status.provider.display_name(),
                "installed": status.installed,
                "version": status.version,
                "authentication": status.authentication.as_str(),
                "authentication_method": status.authentication_method,
                "account_plan": status.account_plan,
                "smoke_eligible": provider_smoke_block_reason(status.provider).is_none(),
                "smoke_block_reason": provider_smoke_block_reason(status.provider),
                "detail": status.detail,
            }))?
        );
    } else {
        println!("provider   {}", status.provider.display_name());
        println!("installed  {}", status.installed);
        println!(
            "version    {}",
            status.version.as_deref().unwrap_or("unavailable")
        );
        println!("auth       {}", status.authentication.as_str());
        if let Some(method) = status.authentication_method {
            println!("method     {method}");
        }
        if let Some(plan) = status.account_plan {
            println!("plan       {plan}");
        }
        if let Some(reason) = provider_smoke_block_reason(status.provider) {
            println!("smoke      blocked · {reason}");
        }
        println!("detail     {}", status.detail);
    }
    Ok(())
}

pub(crate) fn provider_login(
    path: &Path,
    provider: &str,
    device: bool,
) -> Result<(), Box<dyn Error>> {
    let provider = parse_provider(provider)?;
    let flow = if device {
        LoginFlow::Device
    } else {
        LoginFlow::Browser
    };
    match provider {
        DelegatedProviderKind::Codex => println!(
            "Opening the official Codex {} login flow…",
            if device {
                "device-code"
            } else {
                "ChatGPT OAuth"
            }
        ),
        DelegatedProviderKind::Claude => {
            println!("Opening the official Claude Code browser authentication flow…");
        }
        DelegatedProviderKind::Gemini => println!(
            "Opening Gemini CLI authentication. Authentication remains vendor-owned; AER delegated smoke stays blocked until behavior state can be isolated without copying credentials."
        ),
    }
    DelegatedCliProvider::login(provider, path, flow)?;
    let status = DelegatedCliProvider::status(provider, path);
    println!("auth       {}", status.authentication.as_str());
    if let Some(reason) = provider_smoke_block_reason(provider) {
        println!("smoke      blocked · {reason}");
    }
    Ok(())
}

pub(crate) fn provider_logout(path: &Path, provider: &str) -> Result<(), Box<dyn Error>> {
    let provider = parse_provider(provider)?;
    DelegatedCliProvider::logout(provider, path)?;
    println!(
        "{} session cleared by the vendor CLI",
        provider.display_name()
    );
    Ok(())
}

pub(crate) fn provider_smoke(
    path: &Path,
    provider: &str,
    model: Option<String>,
    prompt: &str,
    json: bool,
    show_input: bool,
) -> Result<(), Box<dyn Error>> {
    if prompt.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provider smoke prompt cannot be empty",
        )
        .into());
    }
    let provider = parse_provider(provider)?;
    ensure_provider_smoke_eligible(provider)?;
    let context = ModelContextEnvelope::compile(path, prompt)?;
    let adapter = DelegatedCliProvider::new(
        provider,
        context.rendered.clone(),
        context.digest.clone(),
        model,
    );

    if !json {
        println!("everything provider smoke");
        println!("  provider   {}", provider.display_name());
        println!("  transport  {}", provider.transport());
        println!("  context    {}", short_id(&context.digest));
        println!("  core       {}", short_id(&context.architecture.digest));
        println!("  policy     {}", context.architecture.policy_version);
        println!("  pack       {}", short_id(&context.task_context.pack_id));
        println!(
            "  budget     {} estimated token units · {} selected",
            context.estimated_tokens,
            context.task_context.items.len()
        );
        println!(
            "  sources    {}",
            context.architecture.source_paths().join(", ")
        );
        if !context.task_context.items.is_empty() {
            println!(
                "  retrieved  {}",
                context
                    .task_context
                    .items
                    .iter()
                    .map(|item| item.source_ref.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if show_input {
            println!("\ninput\n-----\n{}", prompt.trim());
        }
        println!("\ncalling model…");
    }

    let trace = adapter.smoke(prompt.trim(), &NeverCancelled)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "provider": trace.provider,
                "transport": trace.transport,
                "model": trace.requested_model.as_deref(),
                "requested_model": trace.requested_model.as_deref(),
                "resolved_models": &trace.resolved_models,
                "provider_cost_usd": trace.provider_cost_usd.as_deref(),
                "provider_request_id": trace.provider_request_id.as_deref(),
                "model_context_digest": trace.architecture_context_digest,
                "model_context_estimated_tokens": context.estimated_tokens,
                "architecture_core": {
                    "version": context.architecture.version,
                    "policy_version": context.architecture.policy_version,
                    "digest": context.architecture.digest,
                    "estimated_tokens": context.architecture.estimated_tokens,
                    "sources": context.architecture.sources.iter().map(|source| serde_json::json!({
                        "path": source.path,
                        "file_sha256": source.sha256,
                        "fragment_sha256": source.fragment_sha256,
                        "section": source.section,
                        "start_line": source.start_line,
                        "end_line": source.end_line,
                        "included_bytes": source.included_bytes,
                        "total_bytes": source.total_bytes,
                    })).collect::<Vec<_>>(),
                },
                "context_pack": {
                    "pack_id": context.task_context.pack_id,
                    "policy_version": context.task_context.policy_version,
                    "repo_snapshot": context.task_context.repo_snapshot,
                    "input_token_budget": context.task_context.input_token_budget,
                    "selected_token_cost": context.task_context.total_token_cost(),
                    "items": context.task_context.items.iter().map(|item| serde_json::json!({
                        "path": item.path,
                        "source_ref": item.source_ref,
                        "content_hash": item.content_hash,
                        "token_cost": item.token_cost,
                        "tier": item.tier.as_u8(),
                        "utility_micros": item.utility_micros,
                    })).collect::<Vec<_>>(),
                    "omitted_high_rank_items": context.task_context.omitted_high_rank_items,
                },
                "input": trace.input,
                "output": trace.output,
                "usage": {
                    "fresh_input_tokens": trace.usage.input_tokens,
                    "cache_creation_input_tokens": trace.usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": trace.usage.cache_read_input_tokens,
                    "output_tokens": trace.usage.output_tokens,
                    "reasoning_output_tokens": trace.usage.reasoning_output_tokens,
                    "exact_observed_input_tokens": trace.usage.exact_observed_input_tokens(),
                },
                "duration_ms": trace.duration_ms,
                "raw_event_count": trace.raw_event_count,
            }))?
        );
        return Ok(());
    }

    println!("\noutput\n------\n{}", trace.output.trim());
    println!("\ntrace");
    println!("  duration   {} ms", trace.duration_ms);
    if !trace.resolved_models.is_empty() {
        println!("  models     {}", trace.resolved_models.join(", "));
    }
    println!(
        "  tokens     fresh {} · cache-write {} · cache-read {} · out {} · reasoning {}",
        display_u64(trace.usage.input_tokens),
        display_u64(trace.usage.cache_creation_input_tokens),
        display_u64(trace.usage.cache_read_input_tokens),
        display_u64(trace.usage.output_tokens),
        display_u64(trace.usage.reasoning_output_tokens),
    );
    if let Some(total) = trace.usage.exact_observed_input_tokens() {
        println!("  observed   {total} exact input tokens");
    }
    if let Some(cost) = &trace.provider_cost_usd {
        println!("  cost       ${cost}");
    }
    if let Some(request_id) = &trace.provider_request_id {
        println!("  request    {request_id}");
    }
    println!("  events     {}", trace.raw_event_count);
    println!(
        "  context    {}",
        short_id(&trace.architecture_context_digest)
    );
    Ok(())
}

pub(crate) fn provider_benchmark(
    path: &Path,
    provider: &str,
    model: Option<String>,
    runs: u8,
    json: bool,
) -> Result<(), Box<dyn Error>> {
    validate_benchmark_runs(runs)?;
    let provider = parse_provider(provider)?;
    ensure_provider_smoke_eligible(provider)?;
    let context = ModelContextEnvelope::compile(path, PROVIDER_CONTEXT_ECONOMICS_INPUT)?;
    let adapter = DelegatedCliProvider::new(
        provider,
        context.rendered.clone(),
        context.digest.clone(),
        model,
    );

    if !json {
        println!("everything provider context economics");
        println!("  benchmark  {PROVIDER_CONTEXT_ECONOMICS_VERSION}");
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
        let trace = adapter.smoke(PROVIDER_CONTEXT_ECONOMICS_INPUT, &NeverCancelled)?;
        let sample = ProviderEconomicsSample::from_trace(
            run,
            &trace,
            PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
        );
        if !json {
            println!(
                "  run {run:<2} fresh {} · write {} · read {} · read-share {} · out {} · {} ms · contract {}",
                display_u64(sample.fresh_input_tokens),
                display_u64(sample.cache_creation_input_tokens),
                display_u64(sample.cache_read_input_tokens),
                display_bps(sample.cache_read_share_bps),
                display_u64(sample.output_tokens),
                sample.duration_ms,
                if sample.output_contract_pass { "PASS" } else { "FAIL" },
            );
        }
        samples.push(sample);
    }

    let report = ProviderEconomicsReport::from_samples(samples);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "benchmark_version": report.version,
                "provider": provider.id(),
                "transport": provider.transport(),
                "runs": runs,
                "canonical_input": PROVIDER_CONTEXT_ECONOMICS_INPUT,
                "expected_output": PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
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
        return Ok(());
    }

    println!("\nmeasurement");
    println!(
        "  integrity  {}",
        if report.measurement_valid { "PASS" } else { "FAIL" }
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
        "  note       input cache economics are reported separately from output tokens and provider-reported total cost"
    );
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProviderEconomicsSample {
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

impl ProviderEconomicsSample {
    fn from_trace(run: u8, trace: &ModelIoTrace, expected_output: &str) -> Self {
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
            output_contract_pass: trace.output.trim() == expected_output,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProviderEconomicsReport {
    version: &'static str,
    samples: Vec<ProviderEconomicsSample>,
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

impl ProviderEconomicsReport {
    fn from_samples(samples: Vec<ProviderEconomicsSample>) -> Self {
        let output_contract_pass = !samples.is_empty()
            && samples.iter().all(|sample| sample.output_contract_pass);
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

        let exact_inputs = collect_complete_u64(&samples, |sample| {
            sample.exact_observed_input_tokens
        });
        let exact_input_min = exact_inputs
            .as_ref()
            .and_then(|values| values.iter().min().copied());
        let exact_input_max = exact_inputs
            .as_ref()
            .and_then(|values| values.iter().max().copied());
        let exact_input_spread_tokens = exact_input_min
            .zip(exact_input_max)
            .and_then(|(minimum, maximum)| maximum.checked_sub(minimum));
        let exact_input_median = exact_inputs
            .as_ref()
            .and_then(|values| median_u64(values));

        let steady = samples.get(1..).unwrap_or(&[]);
        let steady_state_fresh_input_median = collect_complete_u64(steady, |sample| {
            sample.fresh_input_tokens
        })
        .as_ref()
        .and_then(|values| median_u64(values));
        let steady_state_cache_creation_median = collect_complete_u64(steady, |sample| {
            sample.cache_creation_input_tokens
        })
        .as_ref()
        .and_then(|values| median_u64(values));
        let steady_state_cache_read_median = collect_complete_u64(steady, |sample| {
            sample.cache_read_input_tokens
        })
        .as_ref()
        .and_then(|values| median_u64(values));
        let steady_state_fresh_share_bps_median = collect_complete_u32(steady, |sample| {
            sample.fresh_input_share_bps
        })
        .as_ref()
        .and_then(|values| median_u32(values));
        let steady_state_cache_creation_share_bps_median =
            collect_complete_u32(steady, |sample| sample.cache_creation_share_bps)
                .as_ref()
                .and_then(|values| median_u32(values));
        let steady_state_cache_read_share_bps_median = collect_complete_u32(steady, |sample| {
            sample.cache_read_share_bps
        })
        .as_ref()
        .and_then(|values| median_u32(values));

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
            version: PROVIDER_CONTEXT_ECONOMICS_VERSION,
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

fn validate_benchmark_runs(runs: u8) -> Result<(), io::Error> {
    if (MIN_PROVIDER_BENCHMARK_RUNS..=MAX_PROVIDER_BENCHMARK_RUNS).contains(&runs) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "provider benchmark runs must be between {MIN_PROVIDER_BENCHMARK_RUNS} and {MAX_PROVIDER_BENCHMARK_RUNS}"
        ),
    ))
}

fn collect_complete_u64<F>(samples: &[ProviderEconomicsSample], projection: F) -> Option<Vec<u64>>
where
    F: FnMut(&ProviderEconomicsSample) -> Option<u64>,
{
    samples.iter().map(projection).collect()
}

fn collect_complete_u32<F>(samples: &[ProviderEconomicsSample], projection: F) -> Option<Vec<u32>>
where
    F: FnMut(&ProviderEconomicsSample) -> Option<u32>,
{
    samples.iter().map(projection).collect()
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

fn parse_provider(value: &str) -> Result<DelegatedProviderKind, Box<dyn Error>> {
    value.parse::<DelegatedProviderKind>().map_err(Into::into)
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
    use std::ffi::OsString;

    use aer_provider::{
        ProviderUsage,
        delegated::{DelegatedProviderKind, ModelIoTrace},
    };

    use super::{
        PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT, ProviderEconomicsReport,
        ProviderEconomicsSample, contains_provider_command, ensure_provider_smoke_eligible,
        provider_smoke_block_reason, validate_benchmark_runs,
    };

    fn economics_trace(
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
    fn provider_surface_is_lazy_for_ordinary_commands() {
        assert!(!contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("status"),
        ]));
        assert!(!contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("--workspace"),
            OsString::from("provider"),
            OsString::from("status"),
        ]));
        assert!(!contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("intent"),
            OsString::from("provider"),
        ]));
        assert!(contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("--workspace=repo"),
            OsString::from("provider"),
            OsString::from("status"),
            OsString::from("codex"),
        ]));
    }

    #[test]
    fn delegated_smoke_eligibility_is_fail_closed_per_provider() {
        assert!(ensure_provider_smoke_eligible(DelegatedProviderKind::Codex).is_ok());
        assert!(ensure_provider_smoke_eligible(DelegatedProviderKind::Claude).is_ok());
        let error = ensure_provider_smoke_eligible(DelegatedProviderKind::Gemini)
            .expect_err("Gemini delegated smoke must stay blocked");
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("blocked fail-closed"));
        assert!(provider_smoke_block_reason(DelegatedProviderKind::Gemini).is_some());
    }

    #[test]
    fn benchmark_run_count_is_bounded() {
        assert!(validate_benchmark_runs(2).is_ok());
        assert!(validate_benchmark_runs(10).is_ok());
        assert!(validate_benchmark_runs(1).is_err());
        assert!(validate_benchmark_runs(11).is_err());
    }

    #[test]
    fn economics_report_separates_output_variance_from_input_cache_profile() {
        let first = ProviderEconomicsSample::from_trace(
            1,
            &economics_trace(
                "same",
                Some(2),
                Some(6_800),
                Some(4_200),
                1,
                PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
            ),
            PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
        );
        let second = ProviderEconomicsSample::from_trace(
            2,
            &economics_trace(
                "same",
                Some(2),
                Some(1_800),
                Some(9_200),
                4_000,
                PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
            ),
            PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
        );
        let third = ProviderEconomicsSample::from_trace(
            3,
            &economics_trace(
                "same",
                Some(2),
                Some(2_000),
                Some(9_000),
                7,
                PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
            ),
            PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
        );

        let report = ProviderEconomicsReport::from_samples(vec![first, second, third]);
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
    fn economics_report_fails_closed_on_context_drift_or_missing_usage() {
        let first = ProviderEconomicsSample::from_trace(
            1,
            &economics_trace(
                "a",
                Some(2),
                Some(6_800),
                Some(4_200),
                1,
                PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
            ),
            PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
        );
        let second = ProviderEconomicsSample::from_trace(
            2,
            &economics_trace(
                "b",
                None,
                Some(6_800),
                Some(4_200),
                1,
                PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
            ),
            PROVIDER_CONTEXT_ECONOMICS_EXPECTED_OUTPUT,
        );
        let report = ProviderEconomicsReport::from_samples(vec![first, second]);
        assert!(!report.model_context_digest_stable);
        assert!(!report.exact_input_complete);
        assert!(!report.measurement_valid);
    }
}
