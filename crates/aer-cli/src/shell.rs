use std::{
    error::Error,
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use aer_core::{
    default_state_home,
    permissions::{PermissionController, PermissionMode, parse_side_effect},
    spec::{SpecService, SpecSnapshot, UserSemanticKind},
};

use crate::{
    commands::{
        print_doctor, print_intent, print_ir, print_research, print_runs, print_status,
        print_workspace,
    },
    provider_cli::{
        print_providers, provider_login, provider_logout, provider_smoke, provider_status,
    },
};

const HELP: &str = "commands\n  /status                  workspace + spec + runtime summary\n  /workspace               authoritative repository identity\n  /intent                  intent, decisions and open unknowns\n  /ir                      current Engineering IR summary\n  /research                recorded source-backed research evidence\n  /runs                    durable runtime runs\n  /providers               Codex / Claude / Gemini install + auth state\n  /provider status [name]  inspect one provider or all providers\n  /provider login <name>   start official vendor OAuth/auth flow\n  /provider smoke <name> <prompt>  make one real read-only model call\n  /permission              show current autonomy/permission mode\n  /permission <mode>       set plan | default | auto | full for this session\n  /permission allow|deny|reset <effect>  session override\n  /doctor                  explicit heavier environment diagnostic\n\nwrite semantics\n  <text>                    record natural-language user intent\n  /goal <text>             record a user-authoritative goal\n  /non-goal <text>         record a user-authoritative non-goal\n  /constraint <text>       record a user-authoritative constraint\n  /accept <text>           record an observable acceptance criterion\n  /assumption <text>       record a user-authoritative assumption\n  /quality <text>          record a quality attribute\n  /decision <text>         record a user-authoritative decision\n  /research-import <file>  ingest a validated ResearchArtifact JSON file\n\n  /help                    show this list\n  /quit                    exit\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Control {
    Continue,
    Quit,
}

pub(crate) fn run(workspace_root: &Path) -> Result<(), Box<dyn Error>> {
    println!("everything");
    println!("workspace {}", workspace_root.display());
    println!("permission default · reads automatic, other eligible actions ask");
    println!("type /help for available commands");
    println!();

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut line = String::with_capacity(1024);
    let mut permissions = PermissionController::developer_workspace(PermissionMode::Default);

    loop {
        print!("❯ ");
        io::stdout().flush()?;
        line.clear();

        if input.read_line(&mut line)? == 0 {
            println!();
            return Ok(());
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match dispatch(workspace_root, line, &mut permissions) {
            Ok(Control::Continue) => {}
            Ok(Control::Quit) => return Ok(()),
            Err(error) => eprintln!("error: {error}"),
        }
    }
}

fn dispatch(
    workspace_root: &Path,
    input: &str,
    permissions: &mut PermissionController,
) -> Result<Control, Box<dyn Error>> {
    if !input.starts_with('/') {
        let state_home = require_state_home()?;
        let snapshot = SpecService::submit_message(workspace_root, state_home, input)?;
        print_update("intent recorded", &snapshot);
        return Ok(Control::Continue);
    }

    let (command, argument) = split_command(input);
    match command {
        "/help" => print!("{HELP}"),
        "/status" => print_status(workspace_root, false)?,
        "/workspace" => print_workspace(workspace_root, false)?,
        "/intent" => print_intent(workspace_root, false)?,
        "/ir" => print_ir(workspace_root, false)?,
        "/research" => print_research(workspace_root, false)?,
        "/runs" => print_runs(workspace_root, false)?,
        "/providers" => print_providers(workspace_root, false)?,
        "/provider" => handle_provider(workspace_root, argument)?,
        "/permission" => handle_permission(permissions, argument)?,
        "/doctor" => print_doctor(workspace_root, false)?,
        "/goal" => record_semantic(workspace_root, UserSemanticKind::Goal, command, argument)?,
        "/non-goal" => {
            record_semantic(workspace_root, UserSemanticKind::NonGoal, command, argument)?
        }
        "/constraint" => record_semantic(
            workspace_root,
            UserSemanticKind::Constraint,
            command,
            argument,
        )?,
        "/accept" => record_semantic(
            workspace_root,
            UserSemanticKind::AcceptanceCriterion,
            command,
            argument,
        )?,
        "/assumption" => record_semantic(
            workspace_root,
            UserSemanticKind::Assumption,
            command,
            argument,
        )?,
        "/quality" => record_semantic(
            workspace_root,
            UserSemanticKind::QualityAttribute,
            command,
            argument,
        )?,
        "/decision" => record_decision(workspace_root, command, argument)?,
        "/research-import" => import_research(workspace_root, command, argument)?,
        "/quit" => return Ok(Control::Quit),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown command `{command}`; use /help"),
            )
            .into());
        }
    }

    Ok(Control::Continue)
}

fn handle_permission(
    permissions: &mut PermissionController,
    argument: &str,
) -> Result<(), Box<dyn Error>> {
    let parts = argument.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [] => print_permission(permissions),
        [mode] => {
            let mode = mode.parse::<PermissionMode>()?;
            permissions.set_mode(mode);
            print_permission(permissions);
        }
        ["allow", effect] => {
            let effect = parse_side_effect(effect)?;
            permissions.allow_for_session(effect)?;
            println!("permission session override · {effect:?} = allow");
        }
        ["deny", effect] => {
            let effect = parse_side_effect(effect)?;
            permissions.deny_for_session(effect);
            println!("permission session override · {effect:?} = deny");
        }
        ["reset", effect] => {
            let effect = parse_side_effect(effect)?;
            permissions.clear_session_override(effect);
            println!("permission session override cleared · {effect:?}");
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: /permission [plan|default|auto|full] or /permission allow|deny|reset <effect>",
            )
            .into());
        }
    }
    Ok(())
}

fn print_permission(permissions: &PermissionController) {
    let mode = permissions.mode();
    println!("permission {}", mode.as_str());
    println!("  {}", mode.summary());
    println!(
        "  hard ceiling: {}",
        permissions
            .capability_ceiling()
            .iter()
            .map(|effect| format!("{effect:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "  `full` removes prompts inside this ceiling; it does not grant privileged host authority"
    );
}

fn handle_provider(workspace_root: &Path, argument: &str) -> Result<(), Box<dyn Error>> {
    let argument = argument.trim();
    if argument.is_empty() {
        return print_providers(workspace_root, false);
    }
    let mut parts = argument.splitn(3, char::is_whitespace);
    let operation = parts.next().unwrap_or_default();
    let provider = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default().trim();

    match operation {
        "status" if provider.is_empty() => print_providers(workspace_root, false),
        "status" => provider_status(workspace_root, provider, false),
        "login" if !provider.is_empty() => {
            provider_login(workspace_root, provider, rest == "--device")
        }
        "logout" if !provider.is_empty() => provider_logout(workspace_root, provider),
        "smoke" if !provider.is_empty() => provider_smoke(
            workspace_root,
            provider,
            None,
            if rest.is_empty() {
                "State the product name and one architecture rule you were given."
            } else {
                rest
            },
            false,
            true,
        ),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: /provider status [name] | login <name> [--device] | logout <name> | smoke <name> [prompt]",
        )
        .into()),
    }
}

fn record_semantic(
    workspace_root: &Path,
    kind: UserSemanticKind,
    command: &str,
    argument: &str,
) -> Result<(), Box<dyn Error>> {
    let statement = require_argument(command, argument)?;
    let state_home = require_state_home()?;
    let snapshot = SpecService::record_semantic(workspace_root, state_home, kind, statement)?;
    print_update("semantic state updated", &snapshot);
    Ok(())
}

fn record_decision(
    workspace_root: &Path,
    command: &str,
    argument: &str,
) -> Result<(), Box<dyn Error>> {
    let choice = require_argument(command, argument)?;
    let state_home = require_state_home()?;
    let snapshot = SpecService::record_user_decision(workspace_root, state_home, choice)?;
    print_update("decision recorded", &snapshot);
    Ok(())
}

fn import_research(
    workspace_root: &Path,
    command: &str,
    argument: &str,
) -> Result<(), Box<dyn Error>> {
    let raw_path = require_argument(command, argument)?;
    let path = resolve_input_file(workspace_root, strip_quotes(raw_path));
    let bytes = fs::read(&path)?;
    let artifact = serde_json::from_slice(&bytes)?;
    let state_home = require_state_home()?;
    let snapshot = SpecService::ingest_research(workspace_root, state_home, artifact)?;
    print_update("research evidence recorded", &snapshot);
    Ok(())
}

fn print_update(label: &str, snapshot: &SpecSnapshot) {
    println!(
        "{label} · revision {} · {} unknown(s)",
        snapshot.revision,
        snapshot.open_unknown_count()
    );
    if let Some(question) = snapshot.next_question() {
        println!("next: {}", question.question);
    }
}

fn require_state_home() -> Result<PathBuf, Box<dyn Error>> {
    default_state_home().ok_or_else(|| {
        io::Error::other("no platform state directory could be resolved for everything").into()
    })
}

fn require_argument<'a>(command: &str, argument: &'a str) -> Result<&'a str, Box<dyn Error>> {
    if argument.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{command} requires a value"),
        )
        .into());
    }
    Ok(argument.trim())
}

fn split_command(input: &str) -> (&str, &str) {
    match input.find(' ') {
        Some(index) => (&input[..index], input[index + 1..].trim()),
        None => (input, ""),
    }
}

fn resolve_input_file(workspace_root: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    }
}

fn strip_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{HELP, resolve_input_file, split_command, strip_quotes};

    #[test]
    fn command_parser_keeps_the_rest_of_the_line_intact() {
        assert_eq!(
            split_command("/goal build a deterministic runtime"),
            ("/goal", "build a deterministic runtime")
        );
    }

    #[test]
    fn research_path_is_workspace_relative_and_supports_spaces() {
        assert_eq!(
            resolve_input_file(
                Path::new("repo"),
                strip_quotes("\"docs/evidence one.json\"")
            ),
            Path::new("repo").join("docs/evidence one.json")
        );
    }

    #[test]
    fn help_does_not_advertise_removed_ui_only_surfaces() {
        for removed in ["/home", "/settings", "/activity", "/environment", "/clear"] {
            assert!(
                !HELP.contains(removed),
                "removed surface leaked into help: {removed}"
            );
        }
        for live in [
            "/status",
            "/workspace",
            "/intent",
            "/ir",
            "/runs",
            "/providers",
            "/provider",
            "/permission",
            "/doctor",
        ] {
            assert!(
                HELP.contains(live),
                "live capability missing from help: {live}"
            );
        }
    }
}
