use std::{error::Error, path::Path};

use aer_environment::EnvironmentFingerprint;
use aer_workspace::WorkspaceIdentity;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::theme::{Glyphs, Theme};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Home,
    Workspace,
    Environment,
    Providers,
    Activity,
    Settings,
}

impl Screen {
    pub(crate) const ALL: [Self; 6] = [
        Self::Home,
        Self::Workspace,
        Self::Environment,
        Self::Providers,
        Self::Activity,
        Self::Settings,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Workspace => "Workspace",
            Self::Environment => "Environment",
            Self::Providers => "Providers",
            Self::Activity => "Activity",
            Self::Settings => "Settings",
        }
    }

    pub(crate) const fn icon(self, glyphs: &Glyphs) -> &'static str {
        match self {
            Self::Home => glyphs.home,
            Self::Workspace => glyphs.workspace,
            Self::Environment => glyphs.environment,
            Self::Providers => glyphs.providers,
            Self::Activity => glyphs.activity,
            Self::Settings => glyphs.settings,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusTarget {
    Navigation,
    Content,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Overlay {
    None,
    CommandPalette,
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
    OpenCommandPalette,
    OpenProviders,
    OpenSettings,
    OpenActivity,
    NewRun,
    ResumeRun,
    Help,
    Quit,
    Character(char),
    Backspace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaletteIntent {
    Open(Screen),
    Unavailable(&'static str),
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PaletteEntry {
    pub label: &'static str,
    pub hint: &'static str,
    pub intent: PaletteIntent,
}

pub(crate) const PALETTE: &[PaletteEntry] = &[
    PaletteEntry {
        label: "Home",
        hint: "Open the everything home surface",
        intent: PaletteIntent::Open(Screen::Home),
    },
    PaletteEntry {
        label: "Workspace",
        hint: "Inspect repository identity and dirty state",
        intent: PaletteIntent::Open(Screen::Workspace),
    },
    PaletteEntry {
        label: "Environment",
        hint: "Inspect toolchain and dependency fingerprint",
        intent: PaletteIntent::Open(Screen::Environment),
    },
    PaletteEntry {
        label: "Providers",
        hint: "Provider gateway and authentication",
        intent: PaletteIntent::Open(Screen::Providers),
    },
    PaletteEntry {
        label: "Activity",
        hint: "Runtime activity and resumable runs",
        intent: PaletteIntent::Open(Screen::Activity),
    },
    PaletteEntry {
        label: "Settings",
        hint: "Terminal interaction preferences",
        intent: PaletteIntent::Open(Screen::Settings),
    },
    PaletteEntry {
        label: "New run",
        hint: "Available when the single-agent runtime is connected",
        intent: PaletteIntent::Unavailable(
            "New runs are being connected to the single-agent runtime.",
        ),
    },
    PaletteEntry {
        label: "Resume run",
        hint: "Available when resumable runtime state is connected",
        intent: PaletteIntent::Unavailable(
            "Resume is being connected to the single-agent runtime.",
        ),
    },
    PaletteEntry {
        label: "Quit",
        hint: "Leave everything",
        intent: PaletteIntent::Quit,
    },
];

#[derive(Clone, Debug)]
pub struct AppState {
    pub workspace: WorkspaceIdentity,
    pub environment: EnvironmentFingerprint,
    pub theme: Theme,
    pub screen: Screen,
    pub focus: FocusTarget,
    pub overlay: Overlay,
    pub nav_index: usize,
    pub palette_query: String,
    pub palette_index: usize,
    pub should_quit: bool,
    pub notice: Option<String>,
}

impl AppState {
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let workspace = WorkspaceIdentity::inspect(path.as_ref())?;
        let environment = EnvironmentFingerprint::discover(&workspace.repo_root)?;
        Ok(Self {
            workspace,
            environment,
            theme: Theme::discover(),
            screen: Screen::Home,
            focus: FocusTarget::Navigation,
            overlay: Overlay::None,
            nav_index: 0,
            palette_query: String::new(),
            palette_index: 0,
            should_quit: false,
            notice: None,
        })
    }

    pub fn handle(&mut self, action: UiAction) {
        self.notice = None;
        if self.overlay == Overlay::CommandPalette {
            self.handle_palette(action);
            return;
        }
        if self.overlay == Overlay::Help {
            if matches!(action, UiAction::Back | UiAction::Help | UiAction::Confirm) {
                self.overlay = Overlay::None;
            }
            return;
        }

        match action {
            UiAction::MoveUp | UiAction::MoveLeft if self.focus == FocusTarget::Navigation => {
                self.nav_index = self.nav_index.saturating_sub(1);
            }
            UiAction::MoveDown | UiAction::MoveRight if self.focus == FocusTarget::Navigation => {
                self.nav_index = (self.nav_index + 1).min(Screen::ALL.len() - 1);
            }
            UiAction::Confirm if self.focus == FocusTarget::Navigation => {
                self.screen = Screen::ALL[self.nav_index];
                self.focus = FocusTarget::Content;
            }
            UiAction::Back => {
                if self.screen == Screen::Home {
                    self.focus = FocusTarget::Navigation;
                } else {
                    self.screen = Screen::Home;
                    self.nav_index = 0;
                    self.focus = FocusTarget::Navigation;
                }
            }
            UiAction::NextFocus | UiAction::PreviousFocus => {
                self.focus = match self.focus {
                    FocusTarget::Navigation => FocusTarget::Content,
                    FocusTarget::Content => FocusTarget::Navigation,
                };
            }
            UiAction::OpenCommandPalette => self.open_palette(),
            UiAction::OpenProviders => self.open_screen(Screen::Providers),
            UiAction::OpenSettings => self.open_screen(Screen::Settings),
            UiAction::OpenActivity => self.open_screen(Screen::Activity),
            UiAction::Help => self.overlay = Overlay::Help,
            UiAction::NewRun => {
                self.notice = Some("New runs are being connected to the runtime.".to_owned());
            }
            UiAction::ResumeRun => {
                self.notice =
                    Some("Resume is being connected to durable runtime state.".to_owned());
            }
            UiAction::Quit => self.should_quit = true,
            UiAction::MoveUp
            | UiAction::MoveDown
            | UiAction::MoveLeft
            | UiAction::MoveRight
            | UiAction::Confirm
            | UiAction::Character(_)
            | UiAction::Backspace => {}
        }
    }

    fn open_screen(&mut self, screen: Screen) {
        self.screen = screen;
        self.nav_index = Screen::ALL
            .iter()
            .position(|candidate| *candidate == screen)
            .expect("screen is part of navigation");
        self.focus = FocusTarget::Content;
    }

    fn open_palette(&mut self) {
        self.overlay = Overlay::CommandPalette;
        self.palette_query.clear();
        self.palette_index = 0;
    }

    fn handle_palette(&mut self, action: UiAction) {
        match action {
            UiAction::Back | UiAction::OpenCommandPalette => self.overlay = Overlay::None,
            UiAction::MoveUp => self.palette_index = self.palette_index.saturating_sub(1),
            UiAction::MoveDown => {
                let count = self.filtered_palette().len();
                if count > 0 {
                    self.palette_index = (self.palette_index + 1).min(count - 1);
                }
            }
            UiAction::Character(character) => {
                self.palette_query.push(character);
                self.palette_index = 0;
            }
            UiAction::Backspace => {
                self.palette_query.pop();
                self.palette_index = 0;
            }
            UiAction::Confirm => {
                let intent = self
                    .filtered_palette()
                    .get(self.palette_index)
                    .map(|entry| entry.intent);
                match intent {
                    Some(PaletteIntent::Open(screen)) => {
                        self.overlay = Overlay::None;
                        self.open_screen(screen);
                    }
                    Some(PaletteIntent::Unavailable(reason)) => {
                        self.overlay = Overlay::None;
                        self.notice = Some(reason.to_owned());
                    }
                    Some(PaletteIntent::Quit) => {
                        self.overlay = Overlay::None;
                        self.should_quit = true;
                    }
                    None => {}
                }
            }
            UiAction::Help => self.overlay = Overlay::Help,
            UiAction::MoveLeft
            | UiAction::MoveRight
            | UiAction::NextFocus
            | UiAction::PreviousFocus
            | UiAction::OpenProviders
            | UiAction::OpenSettings
            | UiAction::OpenActivity
            | UiAction::NewRun
            | UiAction::ResumeRun
            | UiAction::Quit => {}
        }
    }

    pub(crate) fn filtered_palette(&self) -> Vec<PaletteEntry> {
        let query = self.palette_query.trim().to_ascii_lowercase();
        PALETTE
            .iter()
            .copied()
            .filter(|entry| {
                query.is_empty()
                    || entry.label.to_ascii_lowercase().contains(&query)
                    || entry.hint.to_ascii_lowercase().contains(&query)
            })
            .collect()
    }
}

#[must_use]
pub fn normalize_key(key: KeyEvent, overlay: Overlay) -> Option<UiAction> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if control {
        return match key.code {
            KeyCode::Char('k') => Some(UiAction::OpenCommandPalette),
            KeyCode::Char('p') => Some(UiAction::OpenProviders),
            KeyCode::Char('n') => Some(UiAction::NewRun),
            KeyCode::Char('r') => Some(UiAction::ResumeRun),
            KeyCode::Char('l') => Some(UiAction::OpenActivity),
            KeyCode::Char(',') => Some(UiAction::OpenSettings),
            KeyCode::Char('c') => Some(UiAction::Back),
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
        KeyCode::Backspace if overlay == Overlay::CommandPalette => Some(UiAction::Backspace),
        KeyCode::Char('?') if overlay != Overlay::CommandPalette => Some(UiAction::Help),
        KeyCode::Char('q') if overlay == Overlay::None => Some(UiAction::Quit),
        KeyCode::Char(character) if overlay == Overlay::CommandPalette => {
            Some(UiAction::Character(character))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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
            theme: Theme::test(),
            screen: Screen::Home,
            focus: FocusTarget::Navigation,
            overlay: Overlay::None,
            nav_index: 0,
            palette_query: String::new(),
            palette_index: 0,
            should_quit: false,
            notice: None,
        }
    }

    #[test]
    fn arrows_and_enter_open_selected_surface() {
        let mut app = app();
        app.handle(UiAction::MoveDown);
        app.handle(UiAction::MoveDown);
        app.handle(UiAction::Confirm);
        assert_eq!(app.screen, Screen::Environment);
        assert_eq!(app.focus, FocusTarget::Content);
    }

    #[test]
    fn escape_returns_to_home_and_navigation() {
        let mut app = app();
        app.handle(UiAction::OpenProviders);
        app.handle(UiAction::Back);
        assert_eq!(app.screen, Screen::Home);
        assert_eq!(app.focus, FocusTarget::Navigation);
    }

    #[test]
    fn q_inside_palette_is_text_not_quit() {
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(
            normalize_key(q, Overlay::CommandPalette),
            Some(UiAction::Character('q'))
        );
        assert_eq!(normalize_key(q, Overlay::None), Some(UiAction::Quit));
    }

    #[test]
    fn command_palette_filters_and_opens_provider_surface() {
        let mut app = app();
        app.handle(UiAction::OpenCommandPalette);
        for character in "provider".chars() {
            app.handle(UiAction::Character(character));
        }
        app.handle(UiAction::Confirm);
        assert_eq!(app.overlay, Overlay::None);
        assert_eq!(app.screen, Screen::Providers);
    }
}
