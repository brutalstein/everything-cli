use std::{error::Error, io, path::Path};

use aer_core::model_context::ArchitectureContextCapsule;
use aer_provider::{
    NeverCancelled,
    delegated::{DelegatedCliProvider, DelegatedProviderKind, LoginFlow},
};

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
            "  {:<7} {:<13} {}{}",
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
    }
    println!("\nconnect  everything provider login <codex|claude|gemini>");
    println!("verify   everything provider smoke <provider> --prompt <text>");
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
        DelegatedProviderKind::Codex => {
            println!("Opening the official Codex {} login flow…", if device { "device-code" } else { "ChatGPT OAuth" });
        }
        DelegatedProviderKind::Claude => {
            println!("Opening the official Claude Code browser authentication flow…");
        }
        DelegatedProviderKind::Gemini => {
            println!("Opening Gemini CLI authentication. Choose ‘Sign in with Google’, finish the browser flow, then exit Gemini with /quit.");
        }
    }
    DelegatedCliProvider::login(provider, path, flow)?;
    let status = DelegatedCliProvider::status(provider, path);
    println!("auth       {}", status.authentication.as_str());
    if matches!(provider, DelegatedProviderKind::Gemini) {
        println!("verify     run `everything provider smoke gemini --prompt \"Reply with AER-OK\"`");
    }
    Ok(())
}

pub(crate) fn provider_logout(path: &Path, provider: &str) -> Result<(), Box<dyn Error>> {
    let provider = parse_provider(provider)?;
    DelegatedCliProvider::logout(provider, path)?;
    println!("{} session cleared by the vendor CLI", provider.display_name());
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
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "provider smoke prompt cannot be empty").into());
    }
    let provider = parse_provider(provider)?;
    let capsule = ArchitectureContextCapsule::compile(path)?;
    let adapter = DelegatedCliProvider::new(
        provider,
        path,
        capsule.rendered.clone(),
        capsule.digest.clone(),
        model,
    );

    if !json {
        println!("everything provider smoke");
        println!("  provider   {}", provider.display_name());
        println!("  transport  {}", provider.transport());
        println!("  context    {}", short_id(&capsule.digest));
        println!(
            "  sources    {}",
            capsule
                .sources
                .iter()
                .map(|source| source.path.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
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
                "model": trace.requested_model,
                "architecture_context_digest": trace.architecture_context_digest,
                "architecture_sources": capsule.sources.iter().map(|source| serde_json::json!({
                    "path": source.path,
                    "sha256": source.sha256,
                    "included_bytes": source.included_bytes,
                    "total_bytes": source.total_bytes,
                    "truncated": source.truncated,
                })).collect::<Vec<_>>(),
                "input": trace.input,
                "output": trace.output,
                "usage": {
                    "input_tokens": trace.usage.input_tokens,
                    "output_tokens": trace.usage.output_tokens,
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
    println!(
        "  tokens     in {} · out {}",
        trace
            .usage
            .input_tokens
            .map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
        trace
            .usage
            .output_tokens
            .map_or_else(|| "unknown".to_owned(), |value| value.to_string())
    );
    println!("  events     {}", trace.raw_event_count);
    println!("  context    {}", short_id(&trace.architecture_context_digest));
    Ok(())
}

fn parse_provider(value: &str) -> Result<DelegatedProviderKind, Box<dyn Error>> {
    value.parse::<DelegatedProviderKind>().map_err(Into::into)
}

fn short_id(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}
