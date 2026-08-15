use std::{error::Error, fmt, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlashTarget {
    Home,
    Intent,
    Research,
    EngineeringIr,
    Workspace,
    Environment,
    Providers,
    Activity,
    Settings,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SlashCommand {
    Navigate(SlashTarget),
    Goal(String),
    NonGoal(String),
    Constraint(String),
    Acceptance(String),
    Assumption(String),
    QualityAttribute(String),
    Decision(String),
    ResearchImport(PathBuf),
    Refresh,
    Clear,
    Help,
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlashEntry {
    pub command: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
}

pub const ENTRIES: &[SlashEntry] = &[
    SlashEntry {
        command: "/home",
        usage: "/home",
        description: "Open the product home surface",
    },
    SlashEntry {
        command: "/intent",
        usage: "/intent",
        description: "Inspect authoritative intent, unknowns and decisions",
    },
    SlashEntry {
        command: "/research",
        usage: "/research",
        description: "Inspect source-backed external research evidence",
    },
    SlashEntry {
        command: "/ir",
        usage: "/ir",
        description: "Inspect the current Engineering IR and SpecDelta",
    },
    SlashEntry {
        command: "/workspace",
        usage: "/workspace",
        description: "Inspect repository identity and dirty-state evidence",
    },
    SlashEntry {
        command: "/environment",
        usage: "/environment",
        description: "Inspect environment and dependency identity",
    },
    SlashEntry {
        command: "/providers",
        usage: "/providers",
        description: "Open provider gateway/profile state",
    },
    SlashEntry {
        command: "/activity",
        usage: "/activity",
        description: "Open durable runtime activity",
    },
    SlashEntry {
        command: "/settings",
        usage: "/settings",
        description: "Open terminal/product settings",
    },
    SlashEntry {
        command: "/goal",
        usage: "/goal <statement>",
        description: "Record an explicit user-authoritative goal",
    },
    SlashEntry {
        command: "/non-goal",
        usage: "/non-goal <statement>",
        description: "Record an explicit non-goal",
    },
    SlashEntry {
        command: "/constraint",
        usage: "/constraint <statement>",
        description: "Record an explicit user constraint",
    },
    SlashEntry {
        command: "/accept",
        usage: "/accept <observable criterion>",
        description: "Record an explicit acceptance criterion",
    },
    SlashEntry {
        command: "/assumption",
        usage: "/assumption <statement>",
        description: "Record an explicit assumption",
    },
    SlashEntry {
        command: "/quality",
        usage: "/quality <attribute>",
        description: "Record a quality attribute",
    },
    SlashEntry {
        command: "/decision",
        usage: "/decision <choice>",
        description: "Record an explicit user decision",
    },
    SlashEntry {
        command: "/research-import",
        usage: "/research-import <artifact.json>",
        description: "Ingest a real schema-validated ResearchArtifact JSON file",
    },
    SlashEntry {
        command: "/refresh",
        usage: "/refresh",
        description: "Refresh workspace, runtime and spec projections",
    },
    SlashEntry {
        command: "/clear",
        usage: "/clear",
        description: "Clear the current notice and composer",
    },
    SlashEntry {
        command: "/help",
        usage: "/help",
        description: "Show slash-command and keyboard help",
    },
    SlashEntry {
        command: "/quit",
        usage: "/quit",
        description: "Exit everything",
    },
];

#[must_use]
pub fn suggestions(input: &str) -> Vec<SlashEntry> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') || trimmed.contains(char::is_whitespace) {
        return Vec::new();
    }
    let query = trimmed.to_ascii_lowercase();
    ENTRIES
        .iter()
        .copied()
        .filter(|entry| entry.command.starts_with(&query))
        .collect()
}

pub fn parse(input: &str) -> Result<SlashCommand, SlashError> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return Err(SlashError::NotSlashCommand);
    }
    let (name, argument) = trimmed
        .split_once(char::is_whitespace)
        .map_or((trimmed, ""), |(name, argument)| (name, argument.trim()));
    match name.to_ascii_lowercase().as_str() {
        "/home" => no_argument(argument, SlashCommand::Navigate(SlashTarget::Home)),
        "/intent" => no_argument(argument, SlashCommand::Navigate(SlashTarget::Intent)),
        "/research" => no_argument(argument, SlashCommand::Navigate(SlashTarget::Research)),
        "/ir" => no_argument(argument, SlashCommand::Navigate(SlashTarget::EngineeringIr)),
        "/workspace" => no_argument(argument, SlashCommand::Navigate(SlashTarget::Workspace)),
        "/environment" => no_argument(argument, SlashCommand::Navigate(SlashTarget::Environment)),
        "/providers" => no_argument(argument, SlashCommand::Navigate(SlashTarget::Providers)),
        "/activity" | "/runs" => {
            no_argument(argument, SlashCommand::Navigate(SlashTarget::Activity))
        }
        "/settings" => no_argument(argument, SlashCommand::Navigate(SlashTarget::Settings)),
        "/goal" => text_argument(argument, SlashCommand::Goal),
        "/non-goal" => text_argument(argument, SlashCommand::NonGoal),
        "/constraint" => text_argument(argument, SlashCommand::Constraint),
        "/accept" => text_argument(argument, SlashCommand::Acceptance),
        "/assumption" => text_argument(argument, SlashCommand::Assumption),
        "/quality" => text_argument(argument, SlashCommand::QualityAttribute),
        "/decision" => text_argument(argument, SlashCommand::Decision),
        "/research-import" => {
            let path = required_argument(argument, "/research-import <artifact.json>")?;
            Ok(SlashCommand::ResearchImport(PathBuf::from(unquote(path))))
        }
        "/refresh" => no_argument(argument, SlashCommand::Refresh),
        "/clear" => no_argument(argument, SlashCommand::Clear),
        "/help" => no_argument(argument, SlashCommand::Help),
        "/quit" => no_argument(argument, SlashCommand::Quit),
        unknown => Err(SlashError::UnknownCommand(unknown.to_owned())),
    }
}

fn no_argument(argument: &str, command: SlashCommand) -> Result<SlashCommand, SlashError> {
    if argument.is_empty() {
        Ok(command)
    } else {
        Err(SlashError::UnexpectedArgument)
    }
}

fn text_argument(
    argument: &str,
    constructor: impl FnOnce(String) -> SlashCommand,
) -> Result<SlashCommand, SlashError> {
    Ok(constructor(
        required_argument(argument, "command requires a statement")?.to_owned(),
    ))
}

fn required_argument<'a>(argument: &'a str, usage: &'static str) -> Result<&'a str, SlashError> {
    if argument.trim().is_empty() {
        Err(SlashError::MissingArgument(usage))
    } else {
        Ok(argument.trim())
    }
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed[1..trimmed.len() - 1].to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SlashError {
    NotSlashCommand,
    UnknownCommand(String),
    MissingArgument(&'static str),
    UnexpectedArgument,
}

impl fmt::Display for SlashError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSlashCommand => write!(formatter, "input is not a slash command"),
            Self::UnknownCommand(command) => write!(formatter, "unknown slash command: {command}"),
            Self::MissingArgument(usage) => write!(formatter, "missing argument: {usage}"),
            Self::UnexpectedArgument => write!(formatter, "this slash command takes no argument"),
        }
    }
}

impl Error for SlashError {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{SlashCommand, SlashTarget, parse, suggestions};

    #[test]
    fn navigation_and_semantic_commands_parse_deterministically() {
        assert_eq!(
            parse("/providers").expect("providers"),
            SlashCommand::Navigate(SlashTarget::Providers)
        );
        assert_eq!(
            parse("/goal Preserve user state.").expect("goal"),
            SlashCommand::Goal("Preserve user state.".to_owned())
        );
        assert_eq!(
            parse("/accept verifier exits zero").expect("acceptance"),
            SlashCommand::Acceptance("verifier exits zero".to_owned())
        );
    }

    #[test]
    fn quoted_research_import_preserves_windows_path_with_spaces() {
        assert_eq!(
            parse(r#"/research-import "C:\Users\Cenker\My Artifacts\research.json""#)
                .expect("import"),
            SlashCommand::ResearchImport(PathBuf::from(
                r#"C:\Users\Cenker\My Artifacts\research.json"#
            ))
        );
    }

    #[test]
    fn suggestions_are_prefix_only_and_never_invent_commands() {
        let entries = suggestions("/pro");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].command, "/providers");
        assert!(suggestions("normal text").is_empty());
    }
}
