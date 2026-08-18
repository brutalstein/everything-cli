use std::{
    error::Error,
    fmt::Write as _,
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
    surface::{Role, Status, Surface},
};

const HELP: &str = "commands\n  /status                  workspace + spec + runtime summary\n  /workspace               authoritative repository identity\n  /intent                  intent, decisions and open unknowns\n  /ir                      current Engineering IR summary\n  /research                recorded source-backed research evidence\n  /runs                    durable runtime runs\n  /providers               Codex / Claude / Gemini install + auth state\n  /provider status [name]  inspect one provider or all providers\n  /provider login <name>   start official vendor OAuth/auth flow\n  /provider smoke <name> <prompt>  make one real read-only model call\n  /permission              show current autonomy/permission mode\n  /permission <mode>       set plan | default | auto | full for this session\n  /permission allow|deny|reset <effect>  session override\n  /doctor                  explicit heavier environment diagnostic\n\nwrite semantics\n  <text>                    record natural-language user intent\n  /goal <text>             record a user-authoritative goal\n  /non-goal <text>         record a user-authoritative non-goal\n  /constraint <text>       record a user-authoritative constraint\n  /accept <text>           record an observable acceptance criterion\n  /assumption <text>       record a user-authoritative assumption\n  /quality <text>          record a quality attribute\n  /decision <text>         record a user-authoritative decision\n  /research-import <file>  ingest a validated ResearchArtifact JSON file\n\n  /help                    show this list\n  /quit                    exit\n";

/// Key-column width shared by every aligned field the shell prints.
const FIELD_WIDTH: usize = 11;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Control {
    Continue,
    Quit,
}

pub(crate) fn run(workspace_root: &Path, surface: &Surface) -> Result<(), Box<dyn Error>> {
    let mut permissions = PermissionController::developer_workspace(PermissionMode::Default);
    print!("{}", banner(workspace_root, &permissions, surface));
    io::stdout().flush()?;

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut line = String::with_capacity(1024);
    let prompt = surface.prompt();

    loop {
        print!("{prompt}");
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

        match dispatch(workspace_root, line, &mut permissions, surface) {
            Ok(Control::Continue) => {}
            Ok(Control::Quit) => return Ok(()),
            Err(error) => eprintln!("{} {error}", surface.paint(Role::Failure, "error")),
        }
    }
}

/// The entry screen.
///
/// It reports only what is already known: the workspace path the process was
/// pointed at and the session authority it starts with. Nothing here inspects
/// Git, the environment or the durable runtime, because the first frame must
/// not pay for state the user did not ask for.
fn banner(workspace_root: &Path, permissions: &PermissionController, surface: &Surface) -> String {
    let mode = permissions.mode();
    let mut banner = String::with_capacity(512);
    let _ = writeln!(banner, "{}", surface.heading("everything"));
    let _ = writeln!(
        banner,
        "{}",
        surface.field(
            "workspace",
            &workspace_root.display().to_string(),
            FIELD_WIDTH
        )
    );
    let _ = writeln!(
        banner,
        "{}",
        surface.field(
            "permission",
            &format!("{} · {}", mode.as_str(), mode.summary()),
            FIELD_WIDTH
        )
    );
    let _ = writeln!(banner);
    let _ = writeln!(
        banner,
        "{}",
        surface.paint(Role::Muted, "/help for commands · /quit to exit")
    );
    let _ = writeln!(banner);
    banner
}

fn dispatch(
    workspace_root: &Path,
    input: &str,
    permissions: &mut PermissionController,
    surface: &Surface,
) -> Result<Control, Box<dyn Error>> {
    if !input.starts_with('/') {
        let state_home = require_state_home()?;
        let snapshot = SpecService::submit_message(workspace_root, state_home, input)?;
        print_update("intent recorded", &snapshot);
        return Ok(Control::Continue);
    }

    let (command, argument) = split_command(input);
    match command {
        "/help" => print!("{}", render_help(surface)),
        "/status" => print_status(workspace_root, false, surface)?,
        "/workspace" => print_workspace(workspace_root, false)?,
        "/intent" => print_intent(workspace_root, false)?,
        "/ir" => print_ir(workspace_root, false)?,
        "/research" => print_research(workspace_root, false)?,
        "/runs" => print_runs(workspace_root, false)?,
        "/providers" => print_providers(workspace_root, false)?,
        "/provider" => handle_provider(workspace_root, argument)?,
        "/permission" => handle_permission(permissions, argument, surface)?,
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
            return Err(
                io::Error::new(io::ErrorKind::InvalidInput, unknown_command(command)).into(),
            );
        }
    }

    Ok(Control::Continue)
}

/// Explains an unrecognized command, naming the closest real one when there is
/// an unambiguous candidate.
///
/// The candidates are read out of `HELP` so the suggestion cannot drift away
/// from what the shell documents.
fn unknown_command(command: &str) -> String {
    match closest_command(command) {
        Some(candidate) => {
            format!("unknown command `{command}`; did you mean `{candidate}`? see /help")
        }
        None => format!("unknown command `{command}`; use /help"),
    }
}

/// The documented command sharing the longest prefix with `command`.
///
/// A short prefix match is noise rather than a suggestion, so a candidate must
/// agree on at least three leading characters, and a tie suggests nothing.
fn closest_command(command: &str) -> Option<&'static str> {
    const MIN_SHARED: usize = 3;
    let mut best: Option<(&'static str, usize)> = None;
    let mut tied = false;
    for candidate in HELP
        .split_whitespace()
        .filter(|word| word.starts_with('/') && word.len() > 1)
    {
        let shared = candidate
            .chars()
            .zip(command.chars())
            .take_while(|(left, right)| left == right)
            .count();
        if shared < MIN_SHARED {
            continue;
        }
        match best {
            Some((_, best_shared)) if shared < best_shared => {}
            Some((name, best_shared)) if shared == best_shared => tied = name != candidate,
            _ => {
                best = Some((candidate, shared));
                tied = false;
            }
        }
    }
    if tied {
        None
    } else {
        best.map(|(name, _)| name)
    }
}

fn handle_permission(
    permissions: &mut PermissionController,
    argument: &str,
    surface: &Surface,
) -> Result<(), Box<dyn Error>> {
    let parts = argument.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [] => print!("{}", render_permission(permissions, surface)),
        [mode] => {
            let mode = mode.parse::<PermissionMode>()?;
            permissions.set_mode(mode);
            print!("{}", render_permission(permissions, surface));
        }
        ["allow", effect] => {
            let effect = parse_side_effect(effect)?;
            permissions.allow_for_session(effect)?;
            println!(
                "{}",
                surface.status(
                    Status::Accepted,
                    &format!("session override · {effect:?} allowed"),
                    None
                )
            );
        }
        ["deny", effect] => {
            let effect = parse_side_effect(effect)?;
            permissions.deny_for_session(effect);
            println!(
                "{}",
                surface.status(
                    Status::Blocked,
                    &format!("session override · {effect:?} denied"),
                    None
                )
            );
        }
        ["reset", effect] => {
            let effect = parse_side_effect(effect)?;
            permissions.clear_session_override(effect);
            println!(
                "{}",
                surface.status(
                    Status::Ready,
                    &format!("session override cleared · {effect:?}"),
                    None
                )
            );
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

/// Renders the current authority as a bordered panel.
///
/// Authority is a trust boundary, so it is deliberately framed differently from
/// ordinary informational output rather than folded into the same field list.
fn render_permission(permissions: &PermissionController, surface: &Surface) -> String {
    let mode = permissions.mode();
    let ceiling = permissions
        .capability_ceiling()
        .iter()
        .map(|effect| format!("{effect:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    surface.panel(
        &format!("permission · {}", mode.as_str()),
        &[
            mode.summary().to_owned(),
            format!("hard ceiling: {ceiling}"),
            "`full` removes prompts inside this ceiling; it does not grant privileged host authority"
                .to_owned(),
        ],
    )
}

/// Renders the command list, grouping it the way the user reads it.
///
/// The command names are painted so they separate from their descriptions on a
/// capable terminal, and the plain text is identical everywhere else.
fn render_help(surface: &Surface) -> String {
    let mut help = String::with_capacity(HELP.len() + 256);
    for line in HELP.lines() {
        match line.strip_prefix("  ") {
            Some(entry) if entry.starts_with('/') || entry.starts_with('<') => {
                let (name, description) = entry.split_at(entry.find("  ").unwrap_or(entry.len()));
                let _ = writeln!(
                    help,
                    "  {}{}",
                    surface.paint(Role::Accent, name),
                    surface.paint(Role::Muted, description)
                );
            }
            _ => {
                let _ = writeln!(help, "{}", surface.paint(Role::Neutral, line));
            }
        }
    }
    help
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

    use aer_core::permissions::{PermissionController, PermissionMode};

    use super::{
        HELP, banner, closest_command, render_help, render_permission, resolve_input_file,
        split_command, strip_quotes, unknown_command,
    };
    use crate::surface::Surface;

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

    #[test]
    fn the_entry_screen_states_the_workspace_and_the_authority_it_starts_with() {
        let permissions = PermissionController::developer_workspace(PermissionMode::Default);
        let screen = banner(Path::new("/repo"), &permissions, &Surface::plain());
        assert!(screen.contains("everything"), "{screen}");
        assert!(screen.contains("repo"), "{screen}");
        assert!(screen.contains("default"), "{screen}");
        assert!(screen.contains("/help"), "{screen}");
        assert!(
            !screen.contains('\u{1b}'),
            "a plain entry screen must not paint: {screen:?}"
        );
    }

    #[test]
    fn the_authority_panel_names_the_mode_and_its_hard_ceiling() {
        let permissions = PermissionController::developer_workspace(PermissionMode::Default);
        let panel = render_permission(&permissions, &Surface::plain());
        assert!(panel.contains("permission"), "{panel}");
        assert!(panel.contains("default"), "{panel}");
        assert!(panel.contains("hard ceiling:"), "{panel}");
    }

    #[test]
    fn rendering_help_without_color_changes_no_visible_character() {
        assert_eq!(render_help(&Surface::plain()), HELP);
    }

    #[test]
    fn an_unknown_command_points_at_the_closest_documented_one() {
        assert_eq!(closest_command("/statsu"), Some("/status"));
        assert!(unknown_command("/statsu").contains("did you mean `/status`"));
    }

    #[test]
    fn a_command_with_no_close_match_gets_no_invented_suggestion() {
        assert_eq!(closest_command("/xyzzy"), None);
        let message = unknown_command("/xyzzy");
        assert!(!message.contains("did you mean"), "{message}");
        assert!(message.contains("/help"), "{message}");
    }
}
