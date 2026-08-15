use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use aer_core::{
    RunSummary, default_state_home, list_runs,
    spec::{SpecService, SpecSnapshot, UserSemanticKind},
};
use aer_environment::EnvironmentFingerprint;
use aer_workspace::WorkspaceIdentity;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    slash::{self, SlashCommand, SlashEntry, SlashTarget},
    theme::{Glyphs, Theme},
};

const MAX_HISTORY: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
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

impl Screen {
    pub(crate) const ALL: [Self; 9] = [
        Self::Home,
        Self::Intent,
        Self::Research,
        Self::EngineeringIr,
        Self::Workspace,
        Self::Environment,
        Self::Providers,
        Self::Activity,
        Self::Settings,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Intent => "Intent",
            Self::Research => "Research",
            Self::EngineeringIr => "Engineering IR",
            Self::Workspace => "Workspace",
            Self::Environment => "Environment",
            Self::Providers => "Providers",
            Self::Activity => "Activity",
            Self::Settings => "Settings",
        }
    }

    pub(crate) const fn slash(self) -> &'static str {
        match self {
            Self::Home => "/home",
            Self::Intent => "/intent",
            Self::Research => "/research",
            Self::EngineeringIr => "/ir",
            Self::Workspace => "/workspace",
            Self::Environment => "/environment",
            Self::Providers => "/providers",
            Self::Activity => "/activity",
            Self::Settings => "/settings",
        }
    }

    pub(crate) const fn icon(self, glyphs: &Glyphs) -> &'static str {
        match self {
            Self::Home => glyphs.home,
            Self::Intent => glyphs.intent,
            Self::Research => glyphs.research,
            Self::EngineeringIr => glyphs.engineering_ir,
            Self::Workspace => glyphs.workspace,
            Self::Environment => glyphs.environment,
            Self::Providers => glyphs.providers,
            Self::Activity => glyphs.activity,
            Self::Settings => glyphs.settings,
        }
    }
}

impl From<SlashTarget> for Screen {
    fn from(target: SlashTarget) -> Self {
        match target {
            SlashTarget::Home => Self::Home,
            SlashTarget::Intent => Self::Intent,
            SlashTarget::Research => Self::Research,
            SlashTarget::EngineeringIr => Self::EngineeringIr,
            SlashTarget::Workspace => Self::Workspace,
            SlashTarget::Environment => Self::Environment,
            SlashTarget::Providers => Self::Providers,
            SlashTarget::Activity => Self::Activity,
            SlashTarget::Settings => Self::Settings,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusTarget {
    Composer,
    Navigation,
    Content,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Overlay {
    None,
    Help,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    NextFocus,
    PreviousFocus,
    Confirm,
    Back,
    Character(char),
    Backspace,
    Delete,
    MoveHome,
    MoveEnd,
    Help,
}

#[derive(Clone, Debug)]
pub struct AppState {
    pub workspace: WorkspaceIdentity,
    pub environment: EnvironmentFingerprint,
    pub state_home: Option<PathBuf>,
    pub runs: Vec<RunSummary>,
    pub runtime_error: Option<String>,
    pub spec: Option<SpecSnapshot>,
    pub spec_error: Option<String>,
    pub theme: Theme,
    pub screen: Screen,
    pub focus: FocusTarget,
    pub overlay: Overlay,
    pub nav_index: usize,
    pub composer: String,
    pub composer_cursor: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub slash_index: usize,
    pub should_quit: bool,
    pub notice: Option<String>,
}

impl AppState {
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let workspace = WorkspaceIdentity::inspect(path.as_ref())?;
        let environment = EnvironmentFingerprint::discover(&workspace.repo_root)?;
        let state_home = default_state_home();
        let (runs, runtime_error) = load_runtime_catalog(&workspace.repo_root, state_home.as_deref());
        let (spec, spec_error) = load_spec(&workspace.repo_root, state_home.as_deref());
        Ok(Self {
            workspace,
            environment,
            state_home,
            runs,
            runtime_error,
            spec,
            spec_error,
            theme: Theme::discover(),
            screen: Screen::Home,
            focus: FocusTarget::Composer,
            overlay: Overlay::None,
            nav_index: 0,
            composer: String::new(),
            composer_cursor: 0,
            history: Vec::new(),
            history_index: None,
            slash_index: 0,
            should_quit: false,
            notice: None,
        })
    }

    pub fn handle(&mut self, action: UiAction) {
        if self.overlay == Overlay::Help {
            if matches!(action, UiAction::Back | UiAction::Help | UiAction::Confirm) {
                self.overlay = Overlay::None;
            }
            return;
        }

        match action {
            UiAction::Character(character) => {
                self.focus = FocusTarget::Composer;
                self.insert_char(character);
            }
            UiAction::Backspace => self.backspace(),
            UiAction::Delete => self.delete(),
            UiAction::MoveHome => self.composer_cursor = 0,
            UiAction::MoveEnd => self.composer_cursor = self.composer.chars().count(),
            UiAction::MoveUp => self.move_up(),
            UiAction::MoveDown => self.move_down(),
            UiAction::MoveLeft => self.move_left(),
            UiAction::MoveRight => self.move_right(),
            UiAction::NextFocus => self.focus = next_focus(self.focus),
            UiAction::PreviousFocus => self.focus = previous_focus(self.focus),
            UiAction::Confirm => self.confirm(),
            UiAction::Back => self.back(),
            UiAction::Help => self.overlay = Overlay::Help,
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        for character in text.chars().filter(|character| !character.is_control()) {
            self.insert_char(character);
        }
    }

    pub(crate) fn slash_suggestions(&self) -> Vec<SlashEntry> {
        slash::suggestions(&self.composer)
    }

    pub(crate) fn refresh_all(&mut self) {
        let root = self.workspace.repo_root.clone();
        match WorkspaceIdentity::inspect(&root) {
            Ok(workspace) => {
                self.workspace = workspace;
                match EnvironmentFingerprint::discover(&self.workspace.repo_root) {
                    Ok(environment) => self.environment = environment,
                    Err(error) => self.notice = Some(format!("environment refresh failed: {error}")),
                }
            }
            Err(error) => self.notice = Some(format!("workspace refresh failed: {error}")),
        }
        self.refresh_runtime();
        self.refresh_spec();
    }

    pub(crate) fn refresh_runtime(&mut self) {
        let (runs, runtime_error) =
            load_runtime_catalog(&self.workspace.repo_root, self.state_home.as_deref());
        self.runs = runs;
        self.runtime_error = runtime_error;
    }

    pub(crate) fn refresh_spec(&mut self) {
        let (spec, spec_error) = load_spec(&self.workspace.repo_root, self.state_home.as_deref());
        self.spec = spec;
        self.spec_error = spec_error;
    }

    fn insert_char(&mut self, character: char) {
        if character.is_control() {
            return;
        }
        let byte = char_to_byte_index(&self.composer, self.composer_cursor);
        self.composer.insert(byte, character);
        self.composer_cursor += 1;
        self.history_index = None;
        self.slash_index = 0;
        self.notice = None;
    }

    fn backspace(&mut self) {
        if self.composer_cursor == 0 {
            return;
        }
        let start = char_to_byte_index(&self.composer, self.composer_cursor - 1);
        let end = char_to_byte_index(&self.composer, self.composer_cursor);
        self.composer.replace_range(start..end, "");
        self.composer_cursor -= 1;
        self.history_index = None;
        self.slash_index = 0;
    }

    fn delete(&mut self) {
        if self.composer_cursor >= self.composer.chars().count() {
            return;
        }
        let start = char_to_byte_index(&self.composer, self.composer_cursor);
        let end = char_to_byte_index(&self.composer, self.composer_cursor + 1);
        self.composer.replace_range(start..end, "");
        self.history_index = None;
        self.slash_index = 0;
    }

    fn move_up(&mut self) {
        if self.focus == FocusTarget::Composer {
            let suggestions = self.slash_suggestions();
            if !suggestions.is_empty() {
                self.slash_index = self.slash_index.saturating_sub(1);
                return;
            }
            if !self.history.is_empty() && !self.composer.is_empty() {
                let next = self
                    .history_index
                    .map_or(self.history.len() - 1, |index| index.saturating_sub(1));
                self.set_history(next);
                return;
            }
            if self.composer.is_empty() {
                self.nav_index = self.nav_index.saturating_sub(1);
            }
            return;
        }
        if self.focus == FocusTarget::Navigation {
            self.nav_index = self.nav_index.saturating_sub(1);
        }
    }

    fn move_down(&mut self) {
        if self.focus == FocusTarget::Composer {
            let suggestions = self.slash_suggestions();
            if !suggestions.is_empty() {
                self.slash_index = (self.slash_index + 1).min(suggestions.len() - 1);
                return;
            }
            if let Some(index) = self.history_index {
                if index + 1 < self.history.len() {
                    self.set_history(index + 1);
                } else {
                    self.history_index = None;
                    self.set_composer(String::new());
                }
                return;
            }
            if self.composer.is_empty() {
                self.nav_index = (self.nav_index + 1).min(Screen::ALL.len() - 1);
            }
            return;
        }
        if self.focus == FocusTarget::Navigation {
            self.nav_index = (self.nav_index + 1).min(Screen::ALL.len() - 1);
        }
    }

    fn move_left(&mut self) {
        if self.focus == FocusTarget::Composer && !self.composer.is_empty() {
            self.composer_cursor = self.composer_cursor.saturating_sub(1);
        } else if self.focus == FocusTarget::Navigation {
            self.nav_index = self.nav_index.saturating_sub(1);
        }
    }

    fn move_right(&mut self) {
        if self.focus == FocusTarget::Composer && !self.composer.is_empty() {
            self.composer_cursor =
                (self.composer_cursor + 1).min(self.composer.chars().count());
        } else if self.focus == FocusTarget::Navigation {
            self.nav_index = (self.nav_index + 1).min(Screen::ALL.len() - 1);
        }
    }

    fn confirm(&mut self) {
        if self.focus == FocusTarget::Navigation && self.composer.trim().is_empty() {
            self.open_screen(Screen::ALL[self.nav_index]);
            return;
        }
        if self.composer.trim().is_empty() {
            self.open_screen(Screen::ALL[self.nav_index]);
            return;
        }

        if self.composer.trim_start().starts_with('/')
            && slash::parse(&self.composer).is_err()
        {
            let suggestions = self.slash_suggestions();
            if let Some(entry) = suggestions.get(self.slash_index.min(suggestions.len().saturating_sub(1))) {
                let completion = if entry.usage.contains('<') {
                    format!("{} ", entry.command)
                } else {
                    entry.command.to_owned()
                };
                self.set_composer(completion);
                return;
            }
        }

        let input = self.composer.trim().to_owned();
        self.push_history(input.clone());
        self.set_composer(String::new());
        self.history_index = None;
        self.slash_index = 0;
        if input.starts_with('/') {
            self.execute_slash(&input);
        } else {
            self.submit_message(&input);
        }
    }

    fn execute_slash(&mut self, input: &str) {
        let command = match slash::parse(input) {
            Ok(command) => command,
            Err(error) => {
                self.notice = Some(error.to_string());
                return;
            }
        };
        match command {
            SlashCommand::Navigate(target) => self.open_screen(target.into()),
            SlashCommand::Goal(statement) => {
                self.record_semantic(UserSemanticKind::Goal, &statement, Screen::Intent)
            }
            SlashCommand::NonGoal(statement) => {
                self.record_semantic(UserSemanticKind::NonGoal, &statement, Screen::Intent)
            }
            SlashCommand::Constraint(statement) => {
                self.record_semantic(UserSemanticKind::Constraint, &statement, Screen::Intent)
            }
            SlashCommand::Acceptance(statement) => self.record_semantic(
                UserSemanticKind::AcceptanceCriterion,
                &statement,
                Screen::EngineeringIr,
            ),
            SlashCommand::Assumption(statement) => self.record_semantic(
                UserSemanticKind::Assumption,
                &statement,
                Screen::Intent,
            ),
            SlashCommand::QualityAttribute(statement) => self.record_semantic(
                UserSemanticKind::QualityAttribute,
                &statement,
                Screen::Intent,
            ),
            SlashCommand::Decision(choice) => self.record_decision(&choice),
            SlashCommand::ResearchImport(path) => self.import_research(path),
            SlashCommand::Refresh => {
                self.refresh_all();
                self.notice = Some("Authoritative workspace, runtime and spec projections refreshed.".to_owned());
            }
            SlashCommand::Clear => self.notice = None,
            SlashCommand::Help => self.overlay = Overlay::Help,
            SlashCommand::Quit => self.should_quit = true,
        }
    }

    fn submit_message(&mut self, input: &str) {
        let Some(state_home) = self.state_home.clone() else {
            self.notice = Some("No platform state directory could be resolved.".to_owned());
            return;
        };
        match SpecService::submit_message(&self.workspace.repo_root, state_home, input) {
            Ok(snapshot) => {
                self.spec = Some(snapshot);
                self.spec_error = None;
                self.open_screen(Screen::Intent);
                self.notice = Some(
                    "Intent recorded. No unavailable model extraction was fabricated; inspect /intent and answer explicit unknowns.".to_owned(),
                );
            }
            Err(error) => self.spec_error = Some(error.to_string()),
        }
    }

    fn record_semantic(&mut self, kind: UserSemanticKind, statement: &str, screen: Screen) {
        let Some(state_home) = self.state_home.clone() else {
            self.notice = Some("No platform state directory could be resolved.".to_owned());
            return;
        };
        match SpecService::record_semantic(
            &self.workspace.repo_root,
            state_home,
            kind,
            statement,
        ) {
            Ok(snapshot) => {
                self.spec = Some(snapshot);
                self.spec_error = None;
                self.open_screen(screen);
                self.notice = Some("Authoritative semantic state and Engineering IR updated.".to_owned());
            }
            Err(error) => self.spec_error = Some(error.to_string()),
        }
    }

    fn record_decision(&mut self, choice: &str) {
        let Some(state_home) = self.state_home.clone() else {
            self.notice = Some("No platform state directory could be resolved.".to_owned());
            return;
        };
        match SpecService::record_user_decision(&self.workspace.repo_root, state_home, choice) {
            Ok(snapshot) => {
                self.spec = Some(snapshot);
                self.spec_error = None;
                self.open_screen(Screen::Intent);
                self.notice = Some("User-authoritative decision recorded and IR recompiled.".to_owned());
            }
            Err(error) => self.spec_error = Some(error.to_string()),
        }
    }

    fn import_research(&mut self, path: PathBuf) {
        let Some(state_home) = self.state_home.clone() else {
            self.notice = Some("No platform state directory could be resolved.".to_owned());
            return;
        };
        let result = fs::read(&path)
            .map_err(|error| format!("research import read failed: {error}"))
            .and_then(|bytes| {
                serde_json::from_slice(&bytes)
                    .map_err(|error| format!("research import JSON failed: {error}"))
            })
            .and_then(|artifact| {
                SpecService::ingest_research(&self.workspace.repo_root, state_home, artifact)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(snapshot) => {
                self.spec = Some(snapshot);
                self.spec_error = None;
                self.open_screen(Screen::Research);
                self.notice = Some(
                    "ResearchArtifact validated and recorded as external evidence; no claim was self-promoted to authority."
                        .to_owned(),
                );
            }
            Err(error) => self.notice = Some(error),
        }
    }

    fn open_screen(&mut self, screen: Screen) {
        if screen == Screen::Activity {
            self.refresh_runtime();
        }
        if matches!(screen, Screen::Intent | Screen::Research | Screen::EngineeringIr) {
            self.refresh_spec();
        }
        self.screen = screen;
        self.nav_index = Screen::ALL
            .iter()
            .position(|candidate| *candidate == screen)
            .expect("screen is part of navigation");
        self.focus = FocusTarget::Composer;
    }

    fn back(&mut self) {
        if !self.composer.is_empty() {
            self.set_composer(String::new());
            return;
        }
        if self.screen != Screen::Home {
            self.open_screen(Screen::Home);
        } else {
            self.focus = FocusTarget::Composer;
        }
    }

    fn set_history(&mut self, index: usize) {
        if let Some(value) = self.history.get(index).cloned() {
            self.history_index = Some(index);
            self.set_composer(value);
        }
    }

    fn push_history(&mut self, input: String) {
        if self.history.last() != Some(&input) {
            self.history.push(input);
            if self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }
    }

    fn set_composer(&mut self, value: String) {
        self.composer = value;
        self.composer_cursor = self.composer.chars().count();
        self.slash_index = 0;
    }
}

fn next_focus(focus: FocusTarget) -> FocusTarget {
    match focus {
        FocusTarget::Composer => FocusTarget::Navigation,
        FocusTarget::Navigation => FocusTarget::Content,
        FocusTarget::Content => FocusTarget::Composer,
    }
}

fn previous_focus(focus: FocusTarget) -> FocusTarget {
    match focus {
        FocusTarget::Composer => FocusTarget::Content,
        FocusTarget::Navigation => FocusTarget::Composer,
        FocusTarget::Content => FocusTarget::Navigation,
    }
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

fn load_runtime_catalog(
    workspace_root: &Path,
    state_home: Option<&Path>,
) -> (Vec<RunSummary>, Option<String>) {
    let Some(state_home) = state_home else {
        return (
            Vec::new(),
            Some("No platform state directory could be resolved for everything.".to_owned()),
        );
    };
    match list_runs(workspace_root, state_home) {
        Ok(mut runs) => {
            runs.sort_by(|left, right| right.run_id.cmp(&left.run_id));
            (runs, None)
        }
        Err(error) => (Vec::new(), Some(error.to_string())),
    }
}

fn load_spec(
    workspace_root: &Path,
    state_home: Option<&Path>,
) -> (Option<SpecSnapshot>, Option<String>) {
    let Some(state_home) = state_home else {
        return (
            None,
            Some("No platform state directory could be resolved for specification state.".to_owned()),
        );
    };
    match SpecService::inspect(workspace_root, state_home) {
        Ok(snapshot) => (Some(snapshot), None),
        Err(error) => (None, Some(error.to_string())),
    }
}

#[must_use]
pub fn normalize_key(key: KeyEvent, overlay: Overlay) -> Option<UiAction> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('c') => Some(UiAction::Back),
            _ => None,
        };
    }
    if overlay == Overlay::Help {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::F(1) => Some(UiAction::Back),
            _ => None,
        };
    }
    match key.code {
        KeyCode::Up => Some(UiAction::MoveUp),
        KeyCode::Down => Some(UiAction::MoveDown),
        KeyCode::Left => Some(UiAction::MoveLeft),
        KeyCode::Right => Some(UiAction::MoveRight),
        KeyCode::Enter => Some(UiAction::Confirm),
        KeyCode::Esc => Some(UiAction::Back),
        KeyCode::Tab => Some(UiAction::NextFocus),
        KeyCode::BackTab => Some(UiAction::PreviousFocus),
        KeyCode::Backspace => Some(UiAction::Backspace),
        KeyCode::Delete => Some(UiAction::Delete),
        KeyCode::Home => Some(UiAction::MoveHome),
        KeyCode::End => Some(UiAction::MoveEnd),
        KeyCode::F(1) => Some(UiAction::Help),
        KeyCode::Char(character) => Some(UiAction::Character(character)),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use aer_environment::EnvironmentFingerprint;
    use aer_workspace::WorkspaceIdentity;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{AppState, FocusTarget, Overlay, Screen, UiAction, normalize_key};
    use crate::Theme;

    pub(crate) fn app() -> AppState {
        AppState {
            workspace: WorkspaceIdentity {
                repo_id: "sha256:test".to_owned(),
                repo_root: PathBuf::from("/work/everything-cli"),
                head_commit: "0123456789abcdef".to_owned(),
                branch: Some("main".to_owned()),
                remotes: Vec::new(),
                dirty_tracked_diff_sha256: "tracked".to_owned(),
                untracked_inventory_sha256: "untracked".to_owned(),
                submodule_state_sha256: "submodule".to_owned(),
                tracked_dirty: false,
                untracked_paths: Vec::new(),
            },
            environment: EnvironmentFingerprint {
                os: "windows".to_owned(),
                architecture: "x86_64".to_owned(),
                family: "windows".to_owned(),
                os_version: Some("test".to_owned()),
                shell: Some("pwsh".to_owned()),
                locale: None,
                timezone: None,
                tools: Vec::new(),
                lockfiles: Vec::new(),
                environment_signals: Vec::new(),
                digest: "abcdef0123456789".to_owned(),
            },
            state_home: None,
            runs: Vec::new(),
            runtime_error: None,
            spec: None,
            spec_error: None,
            theme: Theme::test(),
            screen: Screen::Home,
            focus: FocusTarget::Composer,
            overlay: Overlay::None,
            nav_index: 0,
            composer: String::new(),
            composer_cursor: 0,
            history: Vec::new(),
            history_index: None,
            slash_index: 0,
            should_quit: false,
            notice: None,
        }
    }

    #[test]
    fn ordinary_q_is_composer_text_not_a_quit_shortcut() {
        let mut app = app();
        app.handle(UiAction::Character('q'));
        assert_eq!(app.composer, "q");
        assert!(!app.should_quit);
    }

    #[test]
    fn slash_navigation_is_primary_activation_path() {
        let mut app = app();
        app.insert_text("/providers");
        app.handle(UiAction::Confirm);
        assert_eq!(app.screen, Screen::Providers);
        assert!(app.composer.is_empty());
    }

    #[test]
    fn slash_prefix_enter_completes_before_execution() {
        let mut app = app();
        app.insert_text("/pro");
        app.handle(UiAction::Confirm);
        assert_eq!(app.composer, "/providers");
        app.handle(UiAction::Confirm);
        assert_eq!(app.screen, Screen::Providers);
    }

    #[test]
    fn empty_composer_keeps_arrow_navigation_available() {
        let mut app = app();
        app.handle(UiAction::MoveDown);
        app.handle(UiAction::Confirm);
        assert_eq!(app.screen, Screen::Intent);
    }

    #[test]
    fn legacy_ctrl_p_shortcut_is_not_an_activation_authority() {
        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        assert_eq!(normalize_key(key, Overlay::None), None);
    }
}
