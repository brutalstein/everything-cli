use std::{
    io,
    path::{Path, PathBuf},
};

use aer_workspace::WorkspaceIdentity;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{Theme, material_icons};

const HERO: [&str; 5] = [
    "  ___ _   _____ _ __ _   _| |_| |__ (_)_ __   __ _",
    " / _ \\ | / / _ \\ '__| | | | __| '_ \\| | '_ \\ / _` |",
    "|  __/\\ V /  __/ |  | |_| | |_| | | | | | | | (_| |",
    " \\___| \\_/ \\___|_|   \\__, |\\__|_| |_|_|_| |_|\\__, |",
    "                   |___/                     |___/",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LauncherChoice {
    Current,
    Path,
    Quit,
}

impl LauncherChoice {
    const ALL: [Self; 3] = [Self::Current, Self::Path, Self::Quit];

    const fn title(self) -> &'static str {
        match self {
            Self::Current => "Open current repository",
            Self::Path => "Choose another repository",
            Self::Quit => "Quit",
        }
    }

    const fn detail(self) -> &'static str {
        match self {
            Self::Current => "Attach the current Git repository as an everything workspace.",
            Self::Path => "Type a repository path below. No files are changed just by opening it.",
            Self::Quit => "Leave the local terminal client.",
        }
    }

    const fn icon(self) -> &'static str {
        match self {
            Self::Current | Self::Path => material_icons::WORKSPACE.compact,
            Self::Quit => material_icons::ARROW.compact,
        }
    }
}

#[derive(Debug)]
struct LauncherState {
    cwd: PathBuf,
    theme: Theme,
    selected: usize,
    path_input: String,
    path_cursor: usize,
    editing_path: bool,
    notice: Option<String>,
    chosen: Option<PathBuf>,
    should_quit: bool,
}

impl LauncherState {
    fn new(cwd: &Path) -> Self {
        let path_input = cwd.display().to_string();
        let path_cursor = path_input.chars().count();
        Self {
            cwd: cwd.to_path_buf(),
            theme: Theme::discover(),
            selected: 0,
            path_input,
            path_cursor,
            editing_path: false,
            notice: None,
            chosen: None,
            should_quit: false,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.editing_path {
                self.editing_path = false;
                self.notice = None;
            } else {
                self.should_quit = true;
            }
            return;
        }

        if self.editing_path {
            match key.code {
                KeyCode::Enter => self.try_open_typed_path(),
                KeyCode::Esc => {
                    self.editing_path = false;
                    self.notice = None;
                }
                KeyCode::Left => self.path_cursor = self.path_cursor.saturating_sub(1),
                KeyCode::Right => {
                    self.path_cursor = (self.path_cursor + 1).min(self.path_input.chars().count());
                }
                KeyCode::Home => self.path_cursor = 0,
                KeyCode::End => self.path_cursor = self.path_input.chars().count(),
                KeyCode::Backspace => self.path_backspace(),
                KeyCode::Delete => self.path_delete(),
                KeyCode::Char(character) if !character.is_control() => self.path_insert(character),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(LauncherChoice::ALL.len() - 1);
            }
            KeyCode::Enter => self.confirm_choice(),
            KeyCode::Esc => self.should_quit = true,
            KeyCode::Char(character) if !character.is_control() => {
                self.selected = 1;
                self.editing_path = true;
                self.path_input.clear();
                self.path_cursor = 0;
                self.path_insert(character);
            }
            _ => {}
        }
    }

    fn insert_text(&mut self, text: &str) {
        if !self.editing_path {
            self.selected = 1;
            self.editing_path = true;
            self.path_input.clear();
            self.path_cursor = 0;
        }
        for character in text.chars().filter(|character| !character.is_control()) {
            self.path_insert(character);
        }
    }

    fn confirm_choice(&mut self) {
        match LauncherChoice::ALL[self.selected] {
            LauncherChoice::Current => self.try_open(self.cwd.clone()),
            LauncherChoice::Path => {
                self.editing_path = true;
                self.notice = None;
            }
            LauncherChoice::Quit => self.should_quit = true,
        }
    }

    fn try_open_typed_path(&mut self) {
        let raw = self.path_input.trim();
        if raw.is_empty() {
            self.notice = Some("Enter a repository path.".to_owned());
            return;
        }
        self.try_open(resolve_path(&self.cwd, raw));
    }

    fn try_open(&mut self, path: PathBuf) {
        match WorkspaceIdentity::inspect(&path) {
            Ok(workspace) => {
                self.path_input = workspace.repo_root.display().to_string();
                self.path_cursor = self.path_input.chars().count();
                self.chosen = Some(workspace.repo_root);
                self.notice = None;
            }
            Err(error) => {
                self.notice = Some(format!(
                    "Cannot open workspace: {error}. Select a Git working tree; everything does not mutate the folder just to attach it."
                ));
            }
        }
    }

    fn path_insert(&mut self, character: char) {
        let byte = char_to_byte_index(&self.path_input, self.path_cursor);
        self.path_input.insert(byte, character);
        self.path_cursor += 1;
        self.notice = None;
    }

    fn path_backspace(&mut self) {
        if self.path_cursor == 0 {
            return;
        }
        let start = char_to_byte_index(&self.path_input, self.path_cursor - 1);
        let end = char_to_byte_index(&self.path_input, self.path_cursor);
        self.path_input.replace_range(start..end, "");
        self.path_cursor -= 1;
        self.notice = None;
    }

    fn path_delete(&mut self) {
        if self.path_cursor >= self.path_input.chars().count() {
            return;
        }
        let start = char_to_byte_index(&self.path_input, self.path_cursor);
        let end = char_to_byte_index(&self.path_input, self.path_cursor + 1);
        self.path_input.replace_range(start..end, "");
        self.notice = None;
    }
}

pub(crate) fn choose_workspace(start: &Path) -> io::Result<Option<PathBuf>> {
    let mut state = LauncherState::new(start);
    let mut selected = None;
    ratatui::run(|terminal| -> io::Result<()> {
        loop {
            terminal.draw(|frame| render_launcher(frame, &state))?;
            match event::read()? {
                Event::Key(key) => state.handle_key(key),
                Event::Paste(text) => state.insert_text(&text),
                Event::Resize(_, _) | Event::FocusGained | Event::FocusLost | Event::Mouse(_) => {}
            }

            if let Some(path) = state.chosen.take() {
                selected = Some(path);
                return Ok(());
            }
            if state.should_quit {
                return Ok(());
            }
        }
    })?;
    Ok(selected)
}

fn render_launcher(frame: &mut Frame<'_>, state: &LauncherState) {
    let area = frame.area();
    let t = state.theme;
    frame.render_widget(
        Block::default().style(Style::default().bg(t.background).fg(t.text)),
        area,
    );

    let canvas = centered_width(area, 96);
    let root = Layout::vertical([
        Constraint::Length(9),
        Constraint::Length(7),
        Constraint::Min(9),
        Constraint::Length(5),
        Constraint::Length(2),
    ])
    .split(canvas);

    render_hero(frame, root[0], t);
    render_daemon_state(frame, root[1], state);
    render_choices(frame, root[2], state);
    render_path(frame, root[3], state);
    render_footer(frame, root[4], state);
}

fn render_hero(frame: &mut Frame<'_>, area: Rect, t: Theme) {
    if area.width < 68 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "everything",
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "local engineering control plane",
                    Style::default().fg(t.muted),
                )),
            ])
            .alignment(Alignment::Center),
            area,
        );
        return;
    }

    let mut lines = HERO
        .iter()
        .enumerate()
        .map(|(index, line)| {
            Line::from(Span::styled(
                *line,
                Style::default()
                    .fg(if index < 2 { t.accent } else { t.accent_alt })
                    .add_modifier(if index == 0 {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ))
        })
        .collect::<Vec<_>>();
    lines.push(Line::from(vec![
        Span::styled("One CLI for work that spans ", Style::default().fg(t.text)),
        Span::styled(
            "everything.",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn render_daemon_state(frame: &mut Frame<'_>, area: Rect, state: &LauncherState) {
    let t = state.theme;
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!(" {}  DAEMON  ", material_icons::PROVIDERS.compact),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("local runtime", Style::default().fg(t.text)),
            Span::styled("  ·  embedded", Style::default().fg(t.muted)),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {}  workspace  ", material_icons::WORKSPACE.compact),
                Style::default().fg(t.accent_alt),
            ),
            Span::styled("select a Git repository below", Style::default().fg(t.text)),
        ]),
        Line::from(vec![
            Span::styled(
                format!(" {}  provider   ", material_icons::ATTENTION.compact),
                Style::default().fg(t.warning),
            ),
            Span::styled(
                "production profile not configured",
                Style::default().fg(t.muted),
            ),
        ]),
        Line::from(Span::styled(
            "The launcher is a control surface over the existing local runtime; it does not invent a background service.",
            Style::default().fg(t.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(" LOCAL CONTROL PLANE ", t, true))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_choices(frame: &mut Frame<'_>, area: Rect, state: &LauncherState) {
    let t = state.theme;
    let items = LauncherChoice::ALL
        .iter()
        .map(|choice| {
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!(" {}  ", choice.icon()),
                        Style::default().fg(t.accent_alt),
                    ),
                    Span::styled(
                        choice.title(),
                        Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(Span::styled(
                    format!("     {}", choice.detail()),
                    Style::default().fg(t.muted),
                )),
            ])
        })
        .collect::<Vec<_>>();
    let mut list_state = ListState::default().with_selected(Some(state.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(panel(" WORKSPACES ", t, false))
            .highlight_symbol("› ")
            .highlight_style(Style::default().fg(t.accent)),
        area,
        &mut list_state,
    );
}

fn render_path(frame: &mut Frame<'_>, area: Rect, state: &LauncherState) {
    let t = state.theme;
    let (left, right) = split_at_char(&state.path_input, state.path_cursor);
    let input = if state.editing_path {
        Line::from(vec![
            Span::styled(
                " › ",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(left, Style::default().fg(t.text)),
            Span::styled("▏", Style::default().fg(t.accent_alt)),
            Span::styled(right, Style::default().fg(t.text)),
        ])
    } else {
        Line::from(vec![
            Span::styled("   ", Style::default().fg(t.muted)),
            Span::styled(state.path_input.clone(), Style::default().fg(t.muted)),
        ])
    };

    let message = match state.notice.as_deref() {
        Some(value) => value.to_owned(),
        None if state.editing_path => {
            "Enter opens · Esc returns · absolute and relative paths are accepted".to_owned()
        }
        None => "Select “Choose another repository” or start typing a path.".to_owned(),
    };

    frame.render_widget(
        Paragraph::new(vec![
            input,
            Line::from(Span::styled(
                message,
                Style::default().fg(if state.notice.is_some() {
                    t.warning
                } else {
                    t.muted
                }),
            )),
        ])
        .block(panel(" WORKSPACE PATH ", t, state.editing_path))
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &LauncherState) {
    let t = state.theme;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(t.accent_alt)),
            Span::styled(" select   ", Style::default().fg(t.muted)),
            Span::styled("Enter", Style::default().fg(t.accent)),
            Span::styled(" open   ", Style::default().fg(t.muted)),
            Span::styled("type", Style::default().fg(t.accent)),
            Span::styled(" path   ", Style::default().fg(t.muted)),
            Span::styled("Esc/Ctrl+C", Style::default().fg(t.accent_alt)),
            Span::styled(" back/exit", Style::default().fg(t.muted)),
        ]))
        .alignment(Alignment::Center),
        area,
    );
}

fn panel<'a>(title: &'a str, t: Theme, focused: bool) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { t.accent } else { t.border }))
        .style(Style::default().bg(t.panel).fg(t.text))
}

fn centered_width(area: Rect, max_width: u16) -> Rect {
    let width = area.width.min(max_width);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y,
        width,
        height: area.height,
    }
}

fn resolve_path(cwd: &Path, raw: &str) -> PathBuf {
    let expanded = if raw == "~" {
        home_dir().unwrap_or_else(|| cwd.to_path_buf())
    } else if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        home_dir()
            .map(|home| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(raw))
    } else {
        PathBuf::from(raw)
    };
    if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

fn split_at_char(value: &str, char_index: usize) -> (String, String) {
    let byte = char_to_byte_index(value, char_index);
    (value[..byte].to_owned(), value[byte..].to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{LauncherState, resolve_path};

    #[test]
    fn relative_workspace_path_resolves_from_launcher_cwd() {
        let cwd = Path::new("base");
        assert_eq!(
            resolve_path(cwd, "project"),
            PathBuf::from("base").join("project")
        );
    }

    #[test]
    fn launcher_starts_without_claiming_a_workspace() {
        let state = LauncherState::new(Path::new("base"));
        assert!(state.chosen.is_none());
        assert!(!state.should_quit);
    }
}
