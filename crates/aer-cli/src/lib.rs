//! `everything` — keyboard-first terminal product surface.
//!
//! This crate is intentionally a presentation/application adapter. It discovers
//! authoritative workspace/environment state through lower layers and turns
//! terminal key events into typed UI actions. It does not own domain/runtime
//! truth and does not invent provider or run state that does not exist yet.

use std::{
    error::Error,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

use aer_environment::EnvironmentFingerprint;
use aer_workspace::WorkspaceIdentity;
use clap::{Parser, Subcommand};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

const PRODUCT: &str = "everything";
const TAGLINE: &str = "One CLI for work that spans everything.";

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
    const ALL: [Self; 6] = [
        Self::Home,
        Self::Workspace,
        Self::Environment,
        Self::Providers,
        Self::Activity,
        Self::Settings,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Workspace => "Workspace",
            Self::Environment => "Environment",
            Self::Providers => "Providers",
            Self::Activity => "Activity",
            Self::Settings => "Settings",
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
enum PaletteIntent {
    Open(Screen),
    Unavailable(&'static str),
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PaletteEntry {
    label: &'static str,
    hint: &'static str,
    intent: PaletteIntent,
}

const PALETTE: &[PaletteEntry] = &[
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
        hint: "Runtime activity surface",
        intent: PaletteIntent::Open(Screen::Activity),
    },
    PaletteEntry {
        label: "Settings",
        hint: "Terminal interaction preferences",
        intent: PaletteIntent::Open(Screen::Settings),
    },
    PaletteEntry {
        label: "New run",
        hint: "Available when the single-agent runtime lands",
        intent: PaletteIntent::Unavailable("New runs arrive with the single-agent runtime."),
    },
    PaletteEntry {
        label: "Resume run",
        hint: "Available when resumable runtime state lands",
        intent: PaletteIntent::Unavailable("Resume arrives with the single-agent runtime."),
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
                self.notice = Some("New runs arrive with the single-agent runtime.".to_owned());
            }
            UiAction::ResumeRun => {
                self.notice = Some("Resume arrives with the single-agent runtime.".to_owned());
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

    fn filtered_palette(&self) -> Vec<PaletteEntry> {
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

#[derive(Parser, Debug)]
#[command(
    name = "everything",
    version,
    about = "One CLI for work that spans everything."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the current product/workspace status without opening the TUI.
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
    /// Show provider configuration state without claiming a connection.
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
            println!(
                "No provider is configured yet. Provider onboarding is intentionally fail-closed until the provider gateway runtime is available."
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
        println!(
            "{PRODUCT} · {}",
            display_workspace_name(&workspace.repo_root)
        );
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
    ratatui::run(|terminal| {
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

pub fn render(frame: &mut Frame<'_>, app: &AppState) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(if area.height >= 22 { 5 } else { 3 }),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_header(frame, chunks[0]);
    if area.width >= 76 {
        let body =
            Layout::horizontal([Constraint::Length(22), Constraint::Min(32)]).split(chunks[1]);
        render_navigation(frame, body[0], app);
        render_content(frame, body[1], app);
    } else {
        let body = Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).split(chunks[1]);
        render_compact_navigation(frame, body[0], app);
        render_content(frame, body[1], app);
    }
    render_footer(frame, chunks[2], app);
    match app.overlay {
        Overlay::CommandPalette => render_palette(frame, app),
        Overlay::Help => render_help(frame),
        Overlay::None => {}
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect) {
    let title = Line::from(vec![
        Span::styled(
            PRODUCT,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  ·  terminal-native engineering",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let text = if area.height >= 5 {
        vec![title, Line::from(TAGLINE), Line::from("")]
    } else {
        vec![title, Line::from(TAGLINE)]
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::BOTTOM))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_navigation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let items = Screen::ALL
        .iter()
        .map(|screen| ListItem::new(screen.label()))
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.nav_index));
    let focused = app.focus == FocusTarget::Navigation;
    let block = Block::default()
        .title(" navigate ")
        .borders(Borders::RIGHT)
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_symbol("› ")
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        area,
        &mut state,
    );
}

fn render_compact_navigation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let line = Screen::ALL
        .iter()
        .enumerate()
        .flat_map(|(index, screen)| {
            let style = if index == app.nav_index {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            [Span::styled(screen.label(), style), Span::raw("  ")]
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Line::from(line)).wrap(Wrap { trim: true }),
        area,
    );
}

fn render_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let border = if app.focus == FocusTarget::Content {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .title(format!(" {} ", app.screen.label().to_ascii_lowercase()))
        .borders(Borders::ALL)
        .border_style(border);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match app.screen {
        Screen::Home => render_home(frame, inner, app),
        Screen::Workspace => render_workspace(frame, inner, app),
        Screen::Environment => render_environment(frame, inner, app),
        Screen::Providers => render_providers(frame, inner),
        Screen::Activity => render_activity(frame, inner),
        Screen::Settings => render_settings(frame, inner),
    }
}

fn render_home(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let workspace_name = display_workspace_name(&app.workspace.repo_root);
    let state = if app.workspace.is_clean() {
        "clean"
    } else {
        "dirty"
    };
    let branch = app.workspace.branch.as_deref().unwrap_or("detached");
    let env = format!("{} · {}", app.environment.os, app.environment.architecture);
    let lines = vec![
        Line::from(vec![
            Span::styled("Workspace  ", Style::default().fg(Color::DarkGray)),
            Span::styled(workspace_name, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Branch     ", Style::default().fg(Color::DarkGray)),
            Span::raw(branch.to_owned()),
        ]),
        Line::from(vec![
            Span::styled("State      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                state,
                if app.workspace.is_clean() {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("Runtime    ", Style::default().fg(Color::DarkGray)),
            Span::raw("foundation ready"),
        ]),
        Line::from(vec![
            Span::styled("Environment", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("  {env}")),
        ]),
        Line::from(vec![
            Span::styled("Providers  ", Style::default().fg(Color::DarkGray)),
            Span::styled("not configured", Style::default().fg(Color::Magenta)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Use ↑↓ + Enter, or Ctrl+K for the command palette.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn render_workspace(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let lines = vec![
        key_value("root", app.workspace.repo_root.display().to_string()),
        key_value("repo id", short_id(&app.workspace.repo_id)),
        key_value("head", short_id(&app.workspace.head_commit)),
        key_value(
            "branch",
            app.workspace
                .branch
                .clone()
                .unwrap_or_else(|| "detached".to_owned()),
        ),
        key_value(
            "tracked",
            if app.workspace.tracked_dirty {
                "dirty"
            } else {
                "clean"
            },
        ),
        key_value("untracked", app.workspace.untracked_paths.len().to_string()),
        Line::from(""),
        Line::from(Span::styled(
            "User working tree is evidence, never a worker sandbox.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn render_environment(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let mut lines = vec![
        key_value(
            "host",
            format!("{} / {}", app.environment.os, app.environment.architecture),
        ),
        key_value("fingerprint", short_id(&app.environment.digest)),
        key_value("lockfiles", app.environment.lockfiles.len().to_string()),
        Line::from(""),
        Line::from(Span::styled(
            "Tools",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
    ];
    for tool in &app.environment.tools {
        lines.push(key_value(
            &tool.name,
            tool.version
                .clone()
                .unwrap_or_else(|| "unavailable".to_owned()),
        ));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn render_providers(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("No provider connected.", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from("everything will expose one keyboard-first Connect provider flow."),
            Line::from("Official OAuth + PKCE/device flow will be preferred where a provider supports it; otherwise the provider's supported API key/token flow will be used."),
            Line::from(""),
            Line::from(Span::styled("No connection is fabricated before the provider gateway exists.", Style::default().fg(Color::DarkGray))),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_activity(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("No active run.", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from("The activity stream will project authoritative runtime events when the single-agent runtime is added."),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_settings(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            key_value("interaction", "keyboard first"),
            key_value("navigation", "arrows / Enter / Esc / Tab"),
            key_value("palette", "Ctrl+K"),
            key_value("help", "?"),
            key_value("motion", "restrained"),
            Line::from(""),
            Line::from(Span::styled("Persistent settings arrive through the shared configuration runtime; this surface does not invent a second config store.", Style::default().fg(Color::DarkGray))),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let hint = match app.overlay {
        Overlay::CommandPalette => "↑↓ select   Enter open   type filter   Esc close",
        Overlay::Help => "Esc close",
        Overlay::None => {
            "↑↓ navigate   Enter open   Esc back   Tab focus   Ctrl+K commands   ? help   q quit"
        }
    };
    let line = if let Some(notice) = &app.notice {
        Line::from(vec![
            Span::styled("! ", Style::default().fg(Color::Yellow)),
            Span::raw(notice.clone()),
        ])
    } else {
        Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray)))
    };
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}

fn render_palette(frame: &mut Frame<'_>, app: &AppState) {
    let area = centered_rect(
        72.min(frame.area().width.saturating_sub(4)),
        16.min(frame.area().height.saturating_sub(2)),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" command palette · Ctrl+K ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let layout = Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).split(inner);
    frame.render_widget(
        Paragraph::new(format!("> {}", app.palette_query))
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::BOTTOM)),
        layout[0],
    );
    let entries = app.filtered_palette();
    let items = entries
        .iter()
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::styled(entry.label, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("  {}", entry.hint),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(
        (!items.is_empty()).then_some(app.palette_index.min(items.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(Style::default().fg(Color::Cyan)),
        layout[1],
        &mut state,
    );
}

fn render_help(frame: &mut Frame<'_>) {
    let area = centered_rect(
        62.min(frame.area().width.saturating_sub(4)),
        18.min(frame.area().height.saturating_sub(2)),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Keyboard shortcuts",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            key_value("↑ ↓ ← →", "navigate"),
            key_value("Enter", "open / confirm"),
            key_value("Esc", "back / close"),
            key_value("Tab", "change focus"),
            key_value("Ctrl+K", "command palette"),
            key_value("Ctrl+P", "providers"),
            key_value("Ctrl+N", "new run (runtime required)"),
            key_value("Ctrl+L", "activity"),
            key_value("Ctrl+,", "settings"),
            key_value("?", "help"),
            key_value("q", "quit outside text input"),
        ])
        .block(
            Block::default()
                .title(" everything help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn key_value(key: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<12}"), Style::default().fg(Color::DarkGray)),
        Span::raw(value.into()),
    ])
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.max(1).min(area.width);
    let height = height.max(1).min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn display_workspace_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn short_id(value: &str) -> String {
    value.chars().take(14).collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aer_environment::EnvironmentFingerprint;
    use aer_workspace::WorkspaceIdentity;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    use super::{AppState, FocusTarget, Overlay, Screen, UiAction, normalize_key, render};

    fn app() -> AppState {
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
    fn tab_and_backtab_share_deterministic_focus_cycle() {
        let mut app = app();
        app.handle(UiAction::NextFocus);
        assert_eq!(app.focus, FocusTarget::Content);
        app.handle(UiAction::PreviousFocus);
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

    #[test]
    fn wide_and_narrow_render_paths_do_not_panic() {
        for (width, height) in [(100, 30), (52, 20)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            let app = app();
            terminal.draw(|frame| render(frame, &app)).expect("draw");
        }
    }
}
