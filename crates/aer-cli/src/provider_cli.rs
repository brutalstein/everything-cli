use std::{
    env,
    error::Error,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use aer_core::model_context::ModelContextEnvelope;
use aer_provider::{
    NeverCancelled,
    delegated::{DelegatedCliProvider, DelegatedProviderKind, LoginFlow},
};
use clap::{Parser, Subcommand};

mod economics;

const GEMINI_DELEGATED_ISOLATION_BLOCK: &str = "current Gemini CLI delegated OAuth keeps authentication and user behavior/configuration under the same user state; its home .gemini/.env fallback can still inject process configuration even with --ignore-env. AER will not copy OAuth credentials or claim isolation it cannot enforce";

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
            } => economics::run(&workspace, &provider, model, runs, json)?,
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

fn parse_provider(value: &str) -> Result<DelegatedProviderKind, Box<dyn Error>> {
    value.parse::<DelegatedProviderKind>().map_err(Into::into)
}

fn short_id(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

fn display_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use aer_provider::delegated::DelegatedProviderKind;

    use super::{
        contains_provider_command, ensure_provider_smoke_eligible, provider_smoke_block_reason,
    };

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
}
