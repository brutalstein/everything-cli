use std::{
    error::Error,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

use aer_core::{
    RunSummary, default_state_home, list_runs,
    spec::{SpecService, SpecSnapshot},
};
use aer_environment::EnvironmentFingerprint;
use aer_workspace::WorkspaceIdentity;
use clap::{Parser, Subcommand};

use crate::shell;

const PRODUCT: &str = "everything";

#[derive(Parser, Debug)]
#[command(
    name = "everything",
    version,
    about = "Local-first engineering runtime CLI."
)]
struct Cli {
    /// Repository workspace to operate on. Defaults to the current directory.
    #[arg(long, global = true, value_name = "PATH")]
    workspace: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print current workspace, specification and durable runtime status.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Run the explicit, heavier environment/runtime/specification diagnostic.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Inspect authoritative workspace identity.
    Workspace {
        #[arg(long)]
        json: bool,
    },
    /// Inspect authoritative intent, decisions and unknowns.
    Intent {
        #[arg(long)]
        json: bool,
    },
    /// Inspect current Engineering IR and latest SpecDelta.
    Ir {
        #[arg(long)]
        json: bool,
    },
    /// Inspect source-backed research evidence already recorded for the workspace.
    Research {
        #[arg(long)]
        json: bool,
    },
    /// Show durable single-agent runtime runs for the workspace.
    Runs {
        #[arg(long)]
        json: bool,
    },
    /// Show the implemented provider-gateway boundary and current profile state.
    Providers,
}

pub fn run_cli() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let workspace = resolve_workspace(&cwd, cli.workspace.as_deref());

    match cli.command {
        Some(Command::Status { json }) => print_status(&workspace, json),
        Some(Command::Doctor { json }) => print_doctor(&workspace, json),
        Some(Command::Workspace { json }) => print_workspace(&workspace, json),
        Some(Command::Intent { json }) => print_intent(&workspace, json),
        Some(Command::Ir { json }) => print_ir(&workspace, json),
        Some(Command::Research { json }) => print_research(&workspace, json),
        Some(Command::Runs { json }) => print_runs(&workspace, json),
        Some(Command::Providers) => {
            print_providers();
            Ok(())
        }
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => shell::run(&workspace),
        None => print_status(&workspace, false),
    }
}

pub(crate) fn print_status(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let workspace = WorkspaceIdentity::inspect(path)?;
    let runtime = runtime_catalog(&workspace.repo_root);
    let spec = spec_catalog(&workspace.repo_root);

    if json {
        let (runs, runtime_error) = runtime_parts(runtime);
        let (spec, spec_error) = spec_parts(spec);
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "product": PRODUCT,
                "workspace": workspace.repo_root,
                "head": workspace.head_commit,
                "branch": workspace.branch,
                "clean": workspace.is_clean(),
                "provider_gateway_ready": true,
                "provider_configured": false,
                "runtime_runs": runs.len(),
                "latest_run": runs.first().map(run_json),
                "runtime_error": runtime_error,
                "spec_revision": spec.as_ref().map_or(0, |snapshot| snapshot.revision),
                "spec_unknowns": spec.as_ref().map_or(0, SpecSnapshot::open_unknown_count),
                "research_artifacts": spec.as_ref().map_or(0, |snapshot| snapshot.research_artifact_count),
                "spec_error": spec_error,
            }))?
        );
        return Ok(());
    }

    println!("{PRODUCT} · {}", workspace_name(&workspace.repo_root));
    println!(
        "workspace  {}",
        if workspace.is_clean() { "clean" } else { "dirty" }
    );
    println!(
        "branch     {}",
        workspace.branch.as_deref().unwrap_or("detached")
    );
    match spec {
        Ok(spec) => println!(
            "spec       rev {} · {} unknown(s) · {} research artifact(s)",
            spec.revision,
            spec.open_unknown_count(),
            spec.research_artifact_count
        ),
        Err(error) => println!("spec       error · {error}"),
    }
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
    Ok(())
}

pub(crate) fn print_workspace(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
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

pub(crate) fn print_intent(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let spec = spec_catalog(path).map_err(io::Error::other)?;
    let next = spec.next_question();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "revision": spec.revision,
                "messages": spec.intent.messages.len(),
                "goals": spec.intent.goals.len(),
                "non_goals": spec.intent.non_goals.len(),
                "constraints": spec.intent.constraints.len(),
                "assumptions": spec.intent.assumptions.len(),
                "quality_attributes": spec.intent.quality_attributes.len(),
                "acceptance_criteria": spec.intent.acceptance_criteria.len(),
                "user_decisions": spec.intent.user_decisions.len(),
                "unknowns": spec.open_unknown_count(),
                "next_question": next.map(|unknown| serde_json::json!({
                    "id": unknown.id,
                    "question": unknown.question,
                    "question_value": unknown.question_value(),
                    "resolution": format!("{:?}", unknown.resolution).to_ascii_lowercase(),
                })),
            }))?
        );
        return Ok(());
    }

    println!("everything intent · revision {}", spec.revision);
    println!("  messages    {}", spec.intent.messages.len());
    println!("  goals       {}", spec.intent.goals.len());
    println!("  constraints {}", spec.intent.constraints.len());
    println!("  acceptance  {}", spec.intent.acceptance_criteria.len());
    println!("  decisions   {}", spec.intent.user_decisions.len());
    println!("  unknowns    {}", spec.open_unknown_count());
    if let Some(question) = next {
        println!("  next        {}", question.question);
    }
    Ok(())
}

pub(crate) fn print_ir(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let spec = spec_catalog(path).map_err(io::Error::other)?;
    let ir = spec.ir.as_ref();
    let checksum = spec
        .checksum
        .as_ref()
        .map(|checksum| format!("{:?}", checksum.severity).to_ascii_lowercase());

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "revision": spec.revision,
                "compiled": ir.is_some(),
                "semantic_checksum": checksum,
                "goals": ir.map_or(0, |ir| ir.goals.len()),
                "requirements": ir.map_or(0, |ir| ir.functional_requirements.len()),
                "constraints": ir.map_or(0, |ir| ir.constraints.len()),
                "acceptance_criteria": ir.map_or(0, |ir| ir.acceptance_criteria.len()),
                "unknowns": ir.map_or(0, |ir| ir.unknowns.len()),
                "research_findings": ir.map_or(0, |ir| ir.research_findings.len()),
                "latest_delta": spec.latest_delta.as_ref().map(|delta| serde_json::json!({
                    "base_revision": delta.base_revision,
                    "new_revision": delta.new_revision,
                    "added_ids": delta.added_ids,
                    "changed_ids": delta.changed_ids,
                    "invalidated_ids": delta.invalidated_ids,
                })),
            }))?
        );
        return Ok(());
    }

    println!("everything ir · revision {}", spec.revision);
    match ir {
        Some(ir) => {
            println!(
                "  checksum     {}",
                checksum.unwrap_or_else(|| "none".to_owned())
            );
            println!("  goals        {}", ir.goals.len());
            println!("  requirements {}", ir.functional_requirements.len());
            println!("  constraints  {}", ir.constraints.len());
            println!("  acceptance   {}", ir.acceptance_criteria.len());
            println!("  unknowns     {}", ir.unknowns.len());
            println!("  research     {}", ir.research_findings.len());
        }
        None => println!("  no Engineering IR compiled yet"),
    }
    Ok(())
}

pub(crate) fn print_research(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let spec = spec_catalog(path).map_err(io::Error::other)?;
    let findings = spec
        .ir
        .as_ref()
        .map(|ir| ir.research_findings.as_slice())
        .unwrap_or(&[]);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "artifacts": spec.research_artifact_count,
                "claims": findings.iter().map(|finding| serde_json::json!({
                    "research_id": finding.research_id,
                    "claim_id": finding.claim_id,
                    "statement": finding.statement,
                    "status": format!("{:?}", finding.status).to_ascii_lowercase(),
                    "confidence_milli": finding.confidence_milli,
                    "source_refs": finding.source_refs,
                    "authority": "external_evidence",
                })).collect::<Vec<_>>(),
            }))?
        );
        return Ok(());
    }

    println!("everything research");
    println!("  artifacts  {}", spec.research_artifact_count);
    if findings.is_empty() {
        println!("  no source-backed research claims recorded");
    }
    for finding in findings {
        println!(
            "  {}  {:<12}  {}",
            finding.claim_id,
            format!("{:?}", finding.status).to_ascii_lowercase(),
            finding.statement
        );
    }
    Ok(())
}

pub(crate) fn print_runs(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    match runtime_catalog(path) {
        Ok(runs) if json => println!(
            "{}",
            serde_json::to_string_pretty(&runs.iter().map(run_json).collect::<Vec<_>>())?
        ),
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

pub(crate) fn print_doctor(path: &Path, json: bool) -> Result<(), Box<dyn Error>> {
    let workspace = WorkspaceIdentity::inspect(path)?;
    let environment = EnvironmentFingerprint::discover(&workspace.repo_root)?;
    let runtime = runtime_catalog(&workspace.repo_root);
    let spec = spec_catalog(&workspace.repo_root);

    if json {
        let (runs, runtime_error) = runtime_parts(runtime);
        let (spec, spec_error) = spec_parts(spec);
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": runtime_error.is_none() && spec_error.is_none(),
                "workspace": workspace.repo_root,
                "environment_digest": environment.digest,
                "os": environment.os,
                "architecture": environment.architecture,
                "tools": environment.tools.iter().map(|tool| serde_json::json!({
                    "name": tool.name,
                    "version": tool.version
                })).collect::<Vec<_>>(),
                "runtime_runs": runs.len(),
                "runtime_error": runtime_error,
                "spec_revision": spec.as_ref().map_or(0, |snapshot| snapshot.revision),
                "spec_error": spec_error,
            }))?
        );
        return Ok(());
    }

    println!("everything doctor");
    println!("  workspace   ok · {}", workspace.repo_root.display());
    println!(
        "  environment ok · {} / {}",
        environment.os, environment.architecture
    );
    println!("  fingerprint {}", short_id(&environment.digest));
    match spec {
        Ok(spec) => println!("  spec        ok · revision {}", spec.revision),
        Err(error) => println!("  spec        error · {error}"),
    }
    match runtime {
        Ok(runs) => println!("  runtime     ok · {} durable run(s)", runs.len()),
        Err(error) => println!("  runtime     error · {error}"),
    }
    Ok(())
}

pub(crate) fn print_providers() {
    println!("everything providers");
    println!("  gateway  implemented");
    println!("  profile  not configured");
    println!("  auth     production onboarding not implemented yet");
}

fn resolve_workspace(cwd: &Path, configured: Option<&Path>) -> PathBuf {
    match configured {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => cwd.join(path),
        None => cwd.to_path_buf(),
    }
}

fn runtime_catalog(path: &Path) -> Result<Vec<RunSummary>, String> {
    let state_home = default_state_home()
        .ok_or_else(|| "no platform state directory could be resolved".to_owned())?;
    let mut runs = list_runs(path, state_home).map_err(|error| error.to_string())?;
    runs.sort_by(|left, right| right.run_id.cmp(&left.run_id));
    Ok(runs)
}

fn spec_catalog(path: &Path) -> Result<SpecSnapshot, String> {
    let state_home = default_state_home()
        .ok_or_else(|| "no platform state directory could be resolved".to_owned())?;
    SpecService::inspect(path, state_home).map_err(|error| error.to_string())
}

fn runtime_parts(runtime: Result<Vec<RunSummary>, String>) -> (Vec<RunSummary>, Option<String>) {
    match runtime {
        Ok(runs) => (runs, None),
        Err(error) => (Vec::new(), Some(error)),
    }
}

fn spec_parts(spec: Result<SpecSnapshot, String>) -> (Option<SpecSnapshot>, Option<String>) {
    match spec {
        Ok(spec) => (Some(spec), None),
        Err(error) => (None, Some(error)),
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

fn workspace_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn short_id(value: &str) -> String {
    value.chars().take(14).collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::resolve_workspace;

    #[test]
    fn relative_workspace_is_resolved_without_discovery() {
        let cwd = Path::new("base");
        assert_eq!(resolve_workspace(cwd, Some(Path::new("repo"))), cwd.join("repo"));
    }

    #[test]
    fn default_workspace_is_current_directory_without_discovery() {
        let cwd = Path::new("base");
        assert_eq!(resolve_workspace(cwd, None), cwd);
    }
}
