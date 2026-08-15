use std::path::Path;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{AppState, FocusTarget, Overlay, Screen, Theme};

const PRODUCT: &str = "everything";
const TAGLINE: &str = "One CLI for work that spans everything.";

pub fn render(frame: &mut Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(app.theme.background).fg(app.theme.text)),
        area,
    );

    if area.width >= 106 && area.height >= 29 && app.screen == Screen::Home {
        render_premium_home(frame, area, app);
    } else {
        render_application_shell(frame, area, app);
    }

    match app.overlay {
        Overlay::CommandPalette => render_palette(frame, app),
        Overlay::Help => render_help(frame, &app.theme),
        Overlay::None => {}
    }
}

fn render_premium_home(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(22),
        Constraint::Length(2),
    ])
    .split(area);
    render_top_bar(frame, root[0], app);

    let columns = Layout::horizontal([Constraint::Percentage(64), Constraint::Percentage(36)])
        .spacing(2)
        .split(root[1]);
    let left = Layout::vertical([
        Constraint::Length(9),
        Constraint::Length(9),
        Constraint::Min(8),
    ])
    .spacing(1)
    .split(columns[0]);
    let right = Layout::vertical([Constraint::Length(16), Constraint::Min(10)])
        .spacing(1)
        .split(columns[1]);

    render_hero(frame, left[0], app);
    render_workspace_card(frame, left[1], app);
    render_command_card(frame, left[2], app);
    render_surfaces_card(frame, right[0], app);
    render_next_action(frame, right[1], app);
    render_footer(frame, root[2], app);
}

fn render_application_shell(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    render_top_bar(frame, root[0], app);

    if area.width >= 78 {
        let body = Layout::horizontal([Constraint::Length(24), Constraint::Min(36)])
            .spacing(1)
            .split(root[1]);
        render_navigation(frame, body[0], app);
        render_content(frame, body[1], app);
    } else {
        let body = Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).split(root[1]);
        render_compact_navigation(frame, body[0], app);
        render_content(frame, body[1], app);
    }
    render_footer(frame, root[2], app);
}

fn render_top_bar(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let workspace = workspace_name(&app.workspace.repo_root);
    let line = Line::from(vec![
        Span::styled(
            format!(" {} {} ", theme.glyphs.terminal, PRODUCT),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  /  ", Style::default().fg(theme.border)),
        Span::styled(workspace, Style::default().fg(theme.muted)),
        Span::styled("  ·  ", Style::default().fg(theme.border)),
        Span::styled(
            app.screen.label().to_ascii_lowercase(),
            Style::default().fg(theme.accent_alt),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.border)),
            )
            .alignment(Alignment::Left),
        area,
    );
}

fn render_hero(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let hero = vec![
        Line::from(Span::styled(
            "  ___ _   _____ _ __ _   _| |_| |__ (_)_ __   __ _",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            " / _ \\ | / / _ \\ '__| | | | __| '_ \\| | '_ \\ / _` |",
            Style::default().fg(theme.accent),
        )),
        Line::from(Span::styled(
            "|  __/\\ V /  __/ |  | |_| | |_| | | | | | | | (_| |",
            Style::default().fg(theme.accent_alt),
        )),
        Line::from(Span::styled(
            " \\___| \\_/ \\___|_|   \\__, |\\__|_| |_|_|_| |_|\\__, |",
            Style::default().fg(theme.accent_alt),
        )),
        Line::from(Span::styled(
            "                   |___/                     |___/",
            Style::default().fg(theme.muted),
        )),
        Line::from(vec![
            Span::styled(
                "  One CLI for work that spans ",
                Style::default().fg(theme.text),
            ),
            Span::styled(
                "everything.",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(hero).wrap(Wrap { trim: false }), area);
}

fn render_workspace_card(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let workspace = workspace_name(&app.workspace.repo_root);
    let state = if app.workspace.is_clean() {
        "clean"
    } else {
        "dirty"
    };
    let state_style = if app.workspace.is_clean() {
        Style::default().fg(theme.success)
    } else {
        Style::default().fg(theme.warning)
    };
    let rows = vec![
        status_row(
            theme.glyphs.workspace,
            "Workspace",
            workspace,
            theme.accent,
            theme,
        ),
        status_row(
            theme.glyphs.branch,
            "Branch",
            app.workspace.branch.as_deref().unwrap_or("detached"),
            theme.text,
            theme,
        ),
        Line::from(vec![
            Span::styled(format!("  {}  ", theme.glyphs.ready), state_style),
            Span::styled(format!("{:<13}", "State"), Style::default().fg(theme.muted)),
            Span::styled(state, state_style.add_modifier(Modifier::BOLD)),
        ]),
        status_row(
            theme.glyphs.environment,
            "Environment",
            format!("{} / {}", app.environment.os, app.environment.architecture),
            theme.text,
            theme,
        ),
        Line::from(vec![
            Span::styled(
                format!("  {}  ", theme.glyphs.providers),
                Style::default().fg(theme.accent_alt),
            ),
            Span::styled(
                format!("{:<13}", "Providers"),
                Style::default().fg(theme.muted),
            ),
            Span::styled("setup required", Style::default().fg(theme.accent_alt)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(rows)
            .block(card(
                " WORKSPACE ",
                theme,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_command_card(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let items = Screen::ALL
        .iter()
        .enumerate()
        .map(|(index, screen)| {
            let selected = index == app.nav_index;
            let style = if selected {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", screen.icon(&theme.glyphs)),
                    if selected {
                        Style::default().fg(theme.accent_alt)
                    } else {
                        Style::default().fg(theme.muted)
                    },
                ),
                Span::styled(screen.label(), style),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.nav_index));
    frame.render_stateful_widget(
        List::new(items)
            .block(card(
                " COMMAND MENU ",
                theme,
                app.focus == FocusTarget::Navigation,
            ))
            .highlight_symbol("  › ")
            .highlight_style(Style::default().fg(theme.accent)),
        area,
        &mut state,
    );
}

fn render_surfaces_card(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let branch = app.workspace.branch.as_deref().unwrap_or("detached");
    let rows = vec![
        surface_row(
            theme.glyphs.branch,
            "Git workspace",
            format!("{} · {}", branch, short_id(&app.workspace.head_commit)),
            "ready",
            theme.success,
            theme,
        ),
        Line::from(""),
        surface_row(
            theme.glyphs.workspace,
            "Local workspace",
            workspace_name(&app.workspace.repo_root),
            if app.workspace.is_clean() {
                "clean"
            } else {
                "dirty"
            },
            if app.workspace.is_clean() {
                theme.success
            } else {
                theme.warning
            },
            theme,
        ),
        Line::from(""),
        surface_row(
            theme.glyphs.providers,
            "Provider gateway",
            "No profile configured",
            "setup",
            theme.accent_alt,
            theme,
        ),
        Line::from(""),
        surface_row(
            theme.glyphs.shield,
            "Environment",
            short_id(&app.environment.digest),
            "fingerprinted",
            theme.success,
            theme,
        ),
    ];
    frame.render_widget(
        Paragraph::new(rows)
            .block(card(" CONNECTED SURFACES ", theme, false))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_next_action(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {}  ", theme.glyphs.arrow),
                Style::default()
                    .fg(theme.accent_alt)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Connect a provider",
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Configure model access, then start your first durable run.",
            Style::default().fg(theme.muted),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Ctrl+P", Style::default().fg(theme.accent)),
            Span::styled("  Providers    ", Style::default().fg(theme.muted)),
            Span::styled("Ctrl+K", Style::default().fg(theme.accent_alt)),
            Span::styled("  Commands", Style::default().fg(theme.muted)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(" NEXT RECOMMENDED ACTION ", theme, false))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_navigation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let items = Screen::ALL
        .iter()
        .map(|screen| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {}  ", screen.icon(&theme.glyphs)),
                    Style::default().fg(theme.muted),
                ),
                Span::raw(screen.label()),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.nav_index));
    frame.render_stateful_widget(
        List::new(items)
            .block(card(
                " EVERYTHING ",
                theme,
                app.focus == FocusTarget::Navigation,
            ))
            .highlight_symbol(" › ")
            .highlight_style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        area,
        &mut state,
    );
}

fn render_compact_navigation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let line = Screen::ALL
        .iter()
        .enumerate()
        .flat_map(|(index, screen)| {
            let style = if index == app.nav_index {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };
            [
                Span::styled(
                    format!("{} {}", screen.icon(&theme.glyphs), screen.label()),
                    style,
                ),
                Span::raw("   "),
            ]
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Line::from(line)).wrap(Wrap { trim: true }),
        area,
    );
}

fn render_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let title = format!(
        " {} {} ",
        app.screen.icon(&theme.glyphs),
        app.screen.label().to_uppercase()
    );
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(if app.focus == FocusTarget::Content {
                    theme.accent
                } else {
                    theme.accent_alt
                })
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.focus == FocusTarget::Content {
            theme.accent
        } else {
            theme.border
        }))
        .style(Style::default().bg(theme.panel).fg(theme.text));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match app.screen {
        Screen::Home => render_home_content(frame, inner, app),
        Screen::Workspace => render_workspace_content(frame, inner, app),
        Screen::Environment => render_environment_content(frame, inner, app),
        Screen::Providers => render_providers_content(frame, inner, app),
        Screen::Activity => render_activity_content(frame, inner, app),
        Screen::Settings => render_settings_content(frame, inner, app),
    }
}

fn render_home_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "everything",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  /  ", Style::default().fg(theme.border)),
            Span::styled(TAGLINE, Style::default().fg(theme.muted)),
        ]),
        Line::from(""),
        key_value(
            theme.glyphs.workspace,
            "workspace",
            workspace_name(&app.workspace.repo_root),
            theme,
        ),
        key_value(
            theme.glyphs.branch,
            "branch",
            app.workspace.branch.as_deref().unwrap_or("detached"),
            theme,
        ),
        key_value(
            theme.glyphs.environment,
            "environment",
            format!("{} / {}", app.environment.os, app.environment.architecture),
            theme,
        ),
        key_value(theme.glyphs.providers, "providers", "setup required", theme),
        Line::from(""),
        Line::from(Span::styled(
            "Use arrows + Enter, or Ctrl+K for commands.",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn render_workspace_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let lines = vec![
        key_value(
            theme.glyphs.workspace,
            "root",
            app.workspace.repo_root.display().to_string(),
            theme,
        ),
        key_value(
            theme.glyphs.shield,
            "repo id",
            short_id(&app.workspace.repo_id),
            theme,
        ),
        key_value(
            theme.glyphs.branch,
            "head",
            short_id(&app.workspace.head_commit),
            theme,
        ),
        key_value(
            theme.glyphs.branch,
            "branch",
            app.workspace
                .branch
                .clone()
                .unwrap_or_else(|| "detached".to_owned()),
            theme,
        ),
        key_value(
            theme.glyphs.ready,
            "tracked",
            if app.workspace.tracked_dirty {
                "dirty"
            } else {
                "clean"
            },
            theme,
        ),
        key_value(
            theme.glyphs.ready,
            "untracked",
            app.workspace.untracked_paths.len().to_string(),
            theme,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "User working tree is evidence, never a worker sandbox.",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn render_environment_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let mut lines = vec![
        key_value(
            theme.glyphs.environment,
            "host",
            format!("{} / {}", app.environment.os, app.environment.architecture),
            theme,
        ),
        key_value(
            theme.glyphs.shield,
            "fingerprint",
            short_id(&app.environment.digest),
            theme,
        ),
        key_value(
            theme.glyphs.workspace,
            "lockfiles",
            app.environment.lockfiles.len().to_string(),
            theme,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "TOOLS",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
    ];
    for tool in &app.environment.tools {
        lines.push(key_value(
            theme.glyphs.terminal,
            &tool.name,
            tool.version
                .clone()
                .unwrap_or_else(|| "unavailable".to_owned()),
            theme,
        ));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn render_providers_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let lines = vec![
        Line::from(Span::styled(
            format!("{}  Provider gateway", theme.glyphs.providers),
            Style::default()
                .fg(theme.accent_alt)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        key_value(theme.glyphs.ready, "status", "not configured", theme),
        key_value(
            theme.glyphs.shield,
            "credentials",
            "no secrets stored",
            theme,
        ),
        Line::from(""),
        Line::from(
            "Provider setup will use official OAuth flows where a provider supports third-party CLI OAuth; otherwise its supported API-key/token mechanism.",
        ),
        Line::from(""),
        Line::from(Span::styled(
            "No provider connectivity is fabricated before the runtime gateway exists.",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn render_activity_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{}  No active run", theme.glyphs.activity),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(
                "Activity will project durable runtime events and resumable execution state.",
            ),
            Line::from(""),
            Line::from(Span::styled(
                "Ctrl+N · new run    Ctrl+R · resume",
                Style::default().fg(theme.muted),
            )),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_settings_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    frame.render_widget(
        Paragraph::new(vec![
            key_value(
                theme.glyphs.settings,
                "interaction",
                "keyboard first",
                theme,
            ),
            key_value(
                theme.glyphs.arrow,
                "navigation",
                "arrows / Enter / Esc / Tab",
                theme,
            ),
            key_value(theme.glyphs.command, "palette", "Ctrl+K", theme),
            key_value(theme.glyphs.command, "help", "?", theme),
            key_value(
                theme.glyphs.environment,
                "icon fallback",
                "EVERYTHING_ASCII=1",
                theme,
            ),
            Line::from(""),
            Line::from(Span::styled(
                "Colors honor NO_COLOR; enhanced RGB is used only when truecolor is advertised.",
                Style::default().fg(theme.muted),
            )),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let theme = app.theme;
    let hint = match app.overlay {
        Overlay::CommandPalette => "↑↓ select  Enter open  type filter  Esc close",
        Overlay::Help => "Esc close",
        Overlay::None => {
            "↑↓ navigate  Enter open  Esc back  Tab focus  Ctrl+K commands  ? help  q quit"
        }
    };
    let line = if let Some(notice) = &app.notice {
        Line::from(vec![
            Span::styled(
                format!(" {} ", theme.glyphs.attention),
                Style::default().fg(theme.warning),
            ),
            Span::raw(notice.clone()),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " everything ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(theme.muted)),
            Span::styled("   ·   ", Style::default().fg(theme.border)),
            Span::styled(hint, Style::default().fg(theme.muted)),
        ])
    };
    frame.render_widget(
        Paragraph::new(line)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(theme.border)),
            )
            .alignment(Alignment::Left),
        area,
    );
}

fn render_palette(frame: &mut Frame<'_>, app: &AppState) {
    let theme = app.theme;
    let area = centered_rect(
        76.min(frame.area().width.saturating_sub(4)),
        18.min(frame.area().height.saturating_sub(2)),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );
    let block = card(" COMMAND PALETTE · Ctrl+K ", theme, true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let layout = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {}  ", theme.glyphs.command),
                Style::default().fg(theme.accent_alt),
            ),
            Span::styled("> ", Style::default().fg(theme.accent)),
            Span::raw(app.palette_query.clone()),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border)),
        ),
        layout[0],
    );
    let entries = app.filtered_palette();
    let items = entries
        .iter()
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    entry.label,
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", entry.hint),
                    Style::default().fg(theme.muted),
                ),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(
        (!items.is_empty()).then_some(app.palette_index.min(items.len().saturating_sub(1))),
    );
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("  › ")
            .highlight_style(Style::default().fg(theme.accent)),
        layout[1],
        &mut state,
    );
}

fn render_help(frame: &mut Frame<'_>, theme: &Theme) {
    let area = centered_rect(
        66.min(frame.area().width.saturating_sub(4)),
        19.min(frame.area().height.saturating_sub(2)),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Keyboard shortcuts",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            key_value(theme.glyphs.arrow, "arrows", "navigate", *theme),
            key_value(theme.glyphs.command, "Enter", "open / confirm", *theme),
            key_value(theme.glyphs.command, "Esc", "back / close", *theme),
            key_value(theme.glyphs.command, "Tab", "change focus", *theme),
            key_value(theme.glyphs.command, "Ctrl+K", "command palette", *theme),
            key_value(theme.glyphs.providers, "Ctrl+P", "providers", *theme),
            key_value(theme.glyphs.activity, "Ctrl+L", "activity", *theme),
            key_value(theme.glyphs.settings, "Ctrl+,", "settings", *theme),
            key_value(theme.glyphs.command, "?", "help", *theme),
            key_value(theme.glyphs.command, "q", "quit outside text input", *theme),
        ])
        .block(card(" EVERYTHING HELP ", *theme, true))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn card<'a>(title: &'a str, theme: Theme, focused: bool) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(if focused {
                    theme.accent
                } else {
                    theme.accent_alt
                })
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { theme.accent } else { theme.border }))
        .style(Style::default().bg(theme.panel).fg(theme.text))
}

fn status_row(
    icon: &str,
    label: &str,
    value: impl Into<String>,
    value_color: ratatui::style::Color,
    theme: Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {icon}  "), Style::default().fg(theme.accent)),
        Span::styled(format!("{label:<13}"), Style::default().fg(theme.muted)),
        Span::styled(value.into(), Style::default().fg(value_color)),
    ])
}

fn surface_row(
    icon: &str,
    label: &str,
    detail: impl Into<String>,
    status: &str,
    status_color: ratatui::style::Color,
    theme: Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {icon}  "), Style::default().fg(theme.accent)),
        Span::styled(
            format!("{label:<18}"),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", detail.into()),
            Style::default().fg(theme.muted),
        ),
        Span::styled(
            format!("{} {status}", theme.glyphs.ready),
            Style::default().fg(status_color),
        ),
    ])
}

fn key_value(icon: &str, key: &str, value: impl Into<String>, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {icon}  "), Style::default().fg(theme.accent)),
        Span::styled(format!("{key:<13}"), Style::default().fg(theme.muted)),
        Span::styled(value.into(), Style::default().fg(theme.text)),
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
    use std::path::PathBuf;

    use aer_environment::EnvironmentFingerprint;
    use aer_workspace::WorkspaceIdentity;
    use ratatui::{Terminal, backend::TestBackend};

    use super::render;
    use crate::{AppState, FocusTarget, Overlay, Screen, Theme};

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
    fn premium_and_compact_render_paths_do_not_panic() {
        for (width, height) in [(132, 38), (100, 30), (52, 20)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            let app = app();
            terminal.draw(|frame| render(frame, &app)).expect("draw");
        }
    }
}
