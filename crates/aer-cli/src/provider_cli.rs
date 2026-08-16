use std::{
    env,
    error::Error,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use aer_core::model_context::ArchitectureContextCapsule;
use aer_provider::{
    NeverCancelled,
    delegated::{DelegatedCliProvider, DelegatedProviderKind, LoginFlow},
};
use clap::{Parser, Subcommand};

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
    /// Make one real, read-only model call with the bounded architecture capsule.
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
    }
    println!("\nconnect  everything provider login <codex|claude|gemini>");
    println!("verify   everything provider smoke <provider> --show-input --prompt <text>");
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
            "Opening Gemini CLI authentication. Choose ‘Sign in with Google’, finish the browser flow, then exit Gemini with /quit."
        ),
    }
    DelegatedCliProvider::login(provider, path, flow)?;
    let status = DelegatedCliProvider::status(provider, path);
    println!("auth       {}", status.authentication.as_str());
    if matches!(provider, DelegatedProviderKind::Gemini) {
        println!(
            "verify     run `everything provider smoke gemini --prompt \"Reply with AER-OK\"`"
        );
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
    let capsule = ArchitectureContextCapsule::compile(path)?;
    let adapter = DelegatedCliProvider::new(
        provider,
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::contains_provider_command;

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
}
