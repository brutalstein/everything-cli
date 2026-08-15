use std::{
    error::Error,
    io::{self, IsTerminal},
    path::Path,
};

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
    /// Show provider configuration state.
    Providers,
}

pub fn run_cli() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    match cli.command {
        Some(Command::Status { json }) => print_status(&cwd, json),
        Some(Command::Doctor { json }) => print_doctor(&cwd, json),
        Some(Command::Workspace { json }) => print_workspace(&cwd, json),
        Some(Command::Providers) => {
            println!("everything providers");
            println!("  status       not configured");
            println!(
                "  next         open the interactive Providers surface or configure runtime access"
            );
            Ok(())
        }
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => run_tui(&cwd),
        None => print_status(&cwd, false),
    }
}

fn print_status(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let workspace = WorkspaceIdentity::inspect(path)?;
    let environment = EnvironmentFingerprint::discover(&workspace.repo_root)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "product": PRODUCT,
                "workspace": workspace.repo_root,
                "head": workspace.head_commit,
                "branch": workspace.branch,
                "clean": workspace.is_clean(),
                "environment_digest": environment.digest,
                "provider_configured": false
            }))?
        );
    } else {
        println!("{PRODUCT} · {}", workspace_name(&workspace.repo_root));
        println!(
            "workspace  {}",
            if workspace.is_clean() {
                "clean"
            } else {
                "dirty"
            }
        );
        println!(
            "branch     {}",
            workspace.branch.as_deref().unwrap_or("detached")
        );
        println!("provider   not configured");
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
            if workspace.is_clean() {
                "clean"
            } else {
                "dirty"
            }
        );
    }
    Ok(())
}

fn print_doctor(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let workspace = WorkspaceIdentity::inspect(path)?;
    let environment = EnvironmentFingerprint::discover(&workspace.repo_root)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "workspace": workspace.repo_root,
                "environment_digest": environment.digest,
                "os": environment.os,
                "architecture": environment.architecture,
                "tools": environment.tools.iter().map(|tool| serde_json::json!({"name": tool.name, "version": tool.version})).collect::<Vec<_>>()
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
    }
    Ok(())
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
