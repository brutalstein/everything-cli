use std::{
    error::Error,
    io::{self, IsTerminal},
    path::Path,
};

use aer_core::{RunSummary, default_state_home, list_runs};
use aer_environment::EnvironmentFingerprint;
use aer_workspace::WorkspaceIdentity;
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event};

use crate::{AppState, normalize_key, render};

const PRODUCT: &str = "everything";

#[derive(Parser, Debug)]
#[command(
    name = "everything",
    version,
    about = "One CLI for work that spans everything."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print current workspace/product status without opening the TUI.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Validate the local repository and environment boundary.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Inspect workspace identity.
    Workspace {
        #[arg(long)]
        json: bool,
    },
    /// Show durable single-agent runtime runs for this workspace.
    Runs {
        #[arg(long)]
        json: bool,
    },
    /// Show provider gateway and authentication state.
    Providers,
}

pub fn run_cli() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    match cli.command {
        Some(Command::Status { json }) => print_status(&cwd, json),
        Some(Command::Doctor { json }) => print_doctor(&cwd, json),
        Some(Command::Workspace { json }) => print_workspace(&cwd, json),
        Some(Command::Runs { json }) => print_runs(&cwd, json),
        Some(Command::Providers) => {
            println!("everything providers");
            println!("  gateway      ready");
            println!("  profile      not configured");
            println!("  credentials  none stored by this runtime surface");
            println!("  next         configure an authenticated production provider profile");
            Ok(())
        }
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => run_tui(&cwd),
        None => print_status(&cwd, false),
    }
}

fn print_status(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let workspace = WorkspaceIdentity::inspect(path)?;
    let environment = EnvironmentFingerprint::discover(&workspace.repo_root)?;
    let runtime = runtime_catalog(path);
    if json {
        let (runs, runtime_error) = runtime_parts(runtime);
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "product": PRODUCT,
                "workspace": workspace.repo_root,
                "head": workspace.head_commit,
                "branch": workspace.branch,
                "clean": workspace.is_clean(),
                "environment_digest": environment.digest,
                "provider_gateway_ready": true,
                "provider_configured": false,
                "runtime_runs": runs.len(),
                "latest_run": runs.first().map(run_json),
                "runtime_error": runtime_error,
            }))?
        );
    } else {
        println!("{PRODUCT} · {}", workspace_name(&workspace.repo_root));
        println!(
            "workspace  {}",
            if workspace.is_clean() { "clean" } else { "dirty" }
        );
        println!(
            "branch     {}",
            workspace.branch.as_deref().unwrap_or("detached")
        );
        match runtime {
            Ok(runs) => {
                println!("runtime    ready · {} durable run(s)", runs.len());
                if let Some(latest) = runs.first() {
                    println!(
                        "latest     {} · {}",
                        short_id(&latest.run_id),
                        run_state(latest)
                    );
                }
            }
            Err(error) => println!("runtime    error · {error}"),
        }
        println!("provider   gateway ready · profile not configured");
    }
    Ok(())
}

fn print_runs(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    match runtime_catalog(path) {
        Ok(runs) if json => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &runs.iter().map(run_json).collect::<Vec<_>>()
                )?
            );
        }
        Ok(runs) => {
            println!("everything runs");
            if runs.is_empty() {
                println!("  no durable runs for this workspace");
            }
            for run in runs {
                println!(
                    "  {}  {:<11}  accepted={}  interrupted={}  {}",
                    short_id(&run.run_id),
                    run_state(&run),
                    run.accepted,
                    run.interrupted,
                    run.goal
                );
            }
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn print_workspace(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let workspace = WorkspaceIdentity::inspect(path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "repo_id": workspace.repo_id,
                "repo_root": workspace.repo_root,
                "head": workspace.head_commit,
                "branch": workspace.branch,
                "tracked_dirty": workspace.tracked_dirty,
                "untracked_paths": workspace.untracked_paths,
                "dirty_diff_sha256": workspace.dirty_tracked_diff_sha256,
                "untracked_inventory_sha256": workspace.untracked_inventory_sha256,
                "submodule_state_sha256": workspace.submodule_state_sha256
            }))?
        );
    } else {
        println!("repo       {}", workspace.repo_root.display());
        println!("repo id    {}", workspace.repo_id);
        println!("head       {}", short_id(&workspace.head_commit));
        println!(
            "branch     {}",
            workspace.branch.as_deref().unwrap_or("detached")
        );
        println!(
            "state      {}",
            if workspace.is_clean() { "clean" } else { "dirty" }
        );
    }
    Ok(())
}

fn print_doctor(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let workspace = WorkspaceIdentity::inspect(path)?;
    let environment = EnvironmentFingerprint::discover(&workspace.repo_root)?;
    let runtime = runtime_catalog(path);
    if json {
        let (runs, runtime_error) = runtime_parts(runtime);
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": runtime_error.is_none(),
                "workspace": workspace.repo_root,
                "environment_digest": environment.digest,
                "os": environment.os,
                "architecture": environment.architecture,
                "tools": environment.tools.iter().map(|tool| serde_json::json!({"name": tool.name, "version": tool.version})).collect::<Vec<_>>(),
                "runtime_runs": runs.len(),
                "runtime_error": runtime_error,
            }))?
        );
    } else {
        println!("everything doctor");
        println!("  workspace     ok · {}", workspace.repo_root.display());
        println!(
            "  environment   ok · {} / {}",
            environment.os, environment.architecture
        );
        println!("  fingerprint   {}", short_id(&environment.digest));
        match runtime {
            Ok(runs) => println!("  runtime       ok · {} durable run(s)", runs.len()),
            Err(error) => println!("  runtime       error · {error}"),
        }
    }
    Ok(())
}

fn runtime_catalog(path: &Path) -> Result<Vec<RunSummary>, String> {
    let state_home = default_state_home()
        .ok_or_else(|| "no platform state directory could be resolved".to_owned())?;
    let mut runs = list_runs(path, state_home).map_err(|error| error.to_string())?;
    runs.sort_by(|left, right| right.run_id.cmp(&left.run_id));
    Ok(runs)
}

fn runtime_parts(runtime: Result<Vec<RunSummary>, String>) -> (Vec<RunSummary>, Option<String>) {
    match runtime {
        Ok(runs) => (runs, None),
        Err(error) => (Vec::new(), Some(error)),
    }
}

fn run_json(run: &RunSummary) -> serde_json::Value {
    serde_json::json!({
        "run_id": run.run_id,
        "project_id": run.project_id,
        "state": run_state(run),
        "goal": run.goal,
        "worktree_path": run.worktree_path,
        "provider_attempts": run.provider_attempts,
        "verification_success": run.verification_success,
        "accepted": run.accepted,
        "interrupted": run.interrupted,
    })
}

fn run_state(run: &RunSummary) -> String {
    format!("{:?}", run.state).to_ascii_lowercase()
}

fn run_tui(path: &Path) -> Result<(), Box<dyn Error>> {
    let mut app = AppState::discover(path)?;
    ratatui::run(|terminal| -> io::Result<()> {
        loop {
            terminal.draw(|frame| render(frame, &app))?;
            match event::read()? {
                Event::Key(key) => {
                    if let Some(action) = normalize_key(key, app.overlay) {
                        app.handle(action);
                    }
                }
                Event::Resize(_, _) | Event::FocusGained | Event::FocusLost => {}
                Event::Mouse(_) | Event::Paste(_) => {}
            }
            if app.should_quit {
                return Ok(());
            }
        }
    })?;
    Ok(())
}

fn workspace_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn short_id(value: &str) -> String {
    value.chars().take(14).collect()
}
