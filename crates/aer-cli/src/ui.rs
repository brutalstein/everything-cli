use std::path::Path;

use aer_core::RunSummary;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
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
        render_shell(frame, area, app);
    }

    match app.overlay {
        Overlay::CommandPalette => render_palette(frame, app),
        Overlay::Help => render_help(frame, app.theme),
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
    top_bar(frame, root[0], app);

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

    hero(frame, left[0], app.theme);
    workspace_card(frame, left[1], app);
    command_card(frame, left[2], app);
    surfaces_card(frame, right[0], app);
    next_action_card(frame, right[1], app);
    footer(frame, root[2], app);
}

fn render_shell(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .split(area);
    top_bar(frame, root[0], app);

    if area.width >= 78 {
        let body = Layout::horizontal([Constraint::Length(24), Constraint::Min(36)])
            .spacing(1)
            .split(root[1]);
        navigation(frame, body[0], app);
        content(frame, body[1], app);
    } else {
        let body = Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).split(root[1]);
        compact_navigation(frame, body[0], app);
        content(frame, body[1], app);
    }
    footer(frame, root[2], app);
}

fn top_bar(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let workspace = workspace_name(&app.workspace.repo_root);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} {} ", t.glyphs.terminal, PRODUCT),
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  /  ", Style::default().fg(t.border)),
            Span::styled(workspace, Style::default().fg(t.muted)),
            Span::styled("  ·  ", Style::default().fg(t.border)),
            Span::styled(
                app.screen.label().to_ascii_lowercase(),
                Style::default().fg(t.accent_alt),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(t.border)),
        ),
        area,
    );
}

fn hero(frame: &mut Frame<'_>, area: Rect, t: Theme) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "  ___ _   _____ _ __ _   _| |_| |__ (_)_ __   __ _",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                " / _ \\ | / / _ \\ '__| | | | __| '_ \\| | '_ \\ / _` |",
                Style::default().fg(t.accent),
            )),
            Line::from(Span::styled(
                "|  __/\\ V /  __/ |  | |_| | |_| | | | | | | | (_| |",
                Style::default().fg(t.accent_alt),
            )),
            Line::from(Span::styled(
                " \\___| \\_/ \\___|_|   \\__, |\\__|_| |_|_|_| |_|\\__, |",
                Style::default().fg(t.accent_alt),
            )),
            Line::from(Span::styled(
                "                   |___/                     |___/",
                Style::default().fg(t.muted),
            )),
            Line::from(vec![
                Span::styled(
                    "  One CLI for work that spans ",
                    Style::default().fg(t.text),
                ),
                Span::styled(
                    "everything.",
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
            ]),
        ])
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn workspace_card(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let clean = app.workspace.is_clean();
    let runtime = runtime_label(app);
    let rows = vec![
        kv(
            t.glyphs.workspace,
            "Workspace",
            workspace_name(&app.workspace.repo_root),
            t.accent,
            t,
        ),
        kv(
            t.glyphs.branch,
            "Branch",
            app.workspace.branch.as_deref().unwrap_or("detached"),
            t.text,
            t,
        ),
        kv(
            t.glyphs.ready,
            "State",
            if clean { "clean" } else { "dirty" },
            if clean { t.success } else { t.warning },
            t,
        ),
        kv(
            t.glyphs.activity,
            "Runtime",
            runtime,
            if app.runtime_error.is_some() {
                t.danger
            } else {
                t.success
            },
            t,
        ),
        kv(
            t.glyphs.providers,
            "Providers",
            "profile required",
            t.accent_alt,
            t,
        ),
    ];
    frame.render_widget(
        Paragraph::new(rows)
            .block(card(" WORKSPACE ", t, app.focus == FocusTarget::Content))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn command_card(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let items = Screen::ALL
        .iter()
        .map(|screen| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {}  ", screen.icon(&t.glyphs)),
                    Style::default().fg(t.muted),
                ),
                Span::raw(screen.label()),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.nav_index));
    frame.render_stateful_widget(
        List::new(items)
            .block(card(
                " COMMAND MENU ",
                t,
                app.focus == FocusTarget::Navigation,
            ))
            .highlight_symbol("  › ")
            .highlight_style(Style::default().fg(t.accent).add_modifier(Modifier::BOLD)),
        area,
        &mut state,
    );
}

fn surfaces_card(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let runtime_status = if let Some(error) = &app.runtime_error {
        (short_text(error, 22), "error", t.danger)
    } else {
        (
            format!("{} durable run(s)", app.runs.len()),
            "ready",
            t.success,
        )
    };
    let rows = vec![
        surface(
            t.glyphs.branch,
            "Git workspace",
            format!(
                "{} · {}",
                app.workspace.branch.as_deref().unwrap_or("detached"),
                short_id(&app.workspace.head_commit)
            ),
            "ready",
            t.success,
            t,
        ),
        Line::from(""),
        surface(
            t.glyphs.workspace,
            "Local workspace",
            workspace_name(&app.workspace.repo_root),
            if app.workspace.is_clean() {
                "clean"
            } else {
                "dirty"
            },
            if app.workspace.is_clean() {
                t.success
            } else {
                t.warning
            },
            t,
        ),
        Line::from(""),
        surface(
            t.glyphs.activity,
            "Runtime",
            runtime_status.0,
            runtime_status.1,
            runtime_status.2,
            t,
        ),
        Line::from(""),
        surface(
            t.glyphs.providers,
            "Provider gateway",
            "gateway online",
            "auth required",
            t.accent_alt,
            t,
        ),
    ];
    frame.render_widget(
        Paragraph::new(rows)
            .block(card(" CONNECTED SURFACES ", t, false))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn next_action_card(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let (title, detail) = if let Some(run) = app.runs.iter().find(|run| !run.state.is_terminal()) {
        (
            "Inspect resumable run",
            format!("{} · {}", short_id(&run.run_id), run_state(run)),
        )
    } else {
        (
            "Connect a provider",
            "Configure authenticated model access for production runs.".to_owned(),
        )
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!("  {}  ", t.glyphs.arrow),
                    Style::default()
                        .fg(t.accent_alt)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    title,
                    Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                format!("  {detail}"),
                Style::default().fg(t.muted),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Ctrl+P", Style::default().fg(t.accent)),
                Span::styled(" Providers    ", Style::default().fg(t.muted)),
                Span::styled("Ctrl+L", Style::default().fg(t.accent_alt)),
                Span::styled(" Activity", Style::default().fg(t.muted)),
            ]),
        ])
        .block(card(" NEXT RECOMMENDED ACTION ", t, false))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn navigation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let items = Screen::ALL
        .iter()
        .map(|screen| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {}  ", screen.icon(&t.glyphs)),
                    Style::default().fg(t.muted),
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
                t,
                app.focus == FocusTarget::Navigation,
            ))
            .highlight_symbol(" › ")
            .highlight_style(Style::default().fg(t.accent).add_modifier(Modifier::BOLD)),
        area,
        &mut state,
    );
}

fn compact_navigation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let spans = Screen::ALL
        .iter()
        .enumerate()
        .flat_map(|(index, screen)| {
            let style = if index == app.nav_index {
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.muted)
            };
            [
                Span::styled(
                    format!("{} {}", screen.icon(&t.glyphs), screen.label()),
                    style,
                ),
                Span::raw("   "),
            ]
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: true }),
        area,
    );
}

fn content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let title = format!(
        " {} {} ",
        app.screen.icon(&t.glyphs),
        app.screen.label().to_uppercase()
    );
    let block = card(&title, t, app.focus == FocusTarget::Content);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match app.screen {
        Screen::Home => home_content(frame, inner, app),
        Screen::Workspace => workspace_content(frame, inner, app),
        Screen::Environment => environment_content(frame, inner, app),
        Screen::Providers => providers_content(frame, inner, app),
        Screen::Activity => activity_content(frame, inner, app),
        Screen::Settings => settings_content(frame, inner, app),
    }
}

fn home_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    PRODUCT,
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  /  ", Style::default().fg(t.border)),
                Span::styled(TAGLINE, Style::default().fg(t.muted)),
            ]),
            Line::from(""),
            kv(
                t.glyphs.workspace,
                "workspace",
                workspace_name(&app.workspace.repo_root),
                t.text,
                t,
            ),
            kv(
                t.glyphs.activity,
                "runtime",
                runtime_label(app),
                if app.runtime_error.is_some() {
                    t.danger
                } else {
                    t.success
                },
                t,
            ),
            kv(
                t.glyphs.providers,
                "providers",
                "gateway ready · profile required",
                t.accent_alt,
                t,
            ),
            Line::from(""),
            Line::from(Span::styled(
                "Use arrows + Enter, or Ctrl+K for commands.",
                Style::default().fg(t.muted),
            )),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn workspace_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    frame.render_widget(
        Paragraph::new(vec![
            kv(
                t.glyphs.workspace,
                "root",
                app.workspace.repo_root.display().to_string(),
                t.text,
                t,
            ),
            kv(
                t.glyphs.shield,
                "repo id",
                short_id(&app.workspace.repo_id),
                t.text,
                t,
            ),
            kv(
                t.glyphs.branch,
                "head",
                short_id(&app.workspace.head_commit),
                t.text,
                t,
            ),
            kv(
                t.glyphs.branch,
                "branch",
                app.workspace.branch.as_deref().unwrap_or("detached"),
                t.text,
                t,
            ),
            kv(
                t.glyphs.ready,
                "tracked",
                if app.workspace.tracked_dirty {
                    "dirty"
                } else {
                    "clean"
                },
                if app.workspace.tracked_dirty {
                    t.warning
                } else {
                    t.success
                },
                t,
            ),
            kv(
                t.glyphs.ready,
                "untracked",
                app.workspace.untracked_paths.len().to_string(),
                t.text,
                t,
            ),
            Line::from(""),
            Line::from(Span::styled(
                "User working tree is evidence, never a worker sandbox.",
                Style::default().fg(t.muted),
            )),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn environment_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let mut lines = vec![
        kv(
            t.glyphs.environment,
            "host",
            format!("{} / {}", app.environment.os, app.environment.architecture),
            t.text,
            t,
        ),
        kv(
            t.glyphs.shield,
            "fingerprint",
            short_id(&app.environment.digest),
            t.accent,
            t,
        ),
        kv(
            t.glyphs.workspace,
            "lockfiles",
            app.environment.lockfiles.len().to_string(),
            t.text,
            t,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "TOOLS",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )),
    ];
    for tool in &app.environment.tools {
        lines.push(kv(
            t.glyphs.terminal,
            &tool.name,
            tool.version.as_deref().unwrap_or("unavailable"),
            t.text,
            t,
        ));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn providers_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!("{}  Provider gateway", t.glyphs.providers),
                Style::default().fg(t.accent_alt).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            kv(t.glyphs.ok, "gateway", "ready", t.success, t),
            kv(t.glyphs.ready, "profile", "not configured", t.accent_alt, t),
            kv(t.glyphs.shield, "credentials", "no runtime secret stored", t.success, t),
            Line::from(""),
            Line::from("Provider abstraction, normalized failures, bounded retry, and cancellation are active. Production model access remains disabled until an authenticated provider profile exists."),
            Line::from(""),
            Line::from(Span::styled(
                "The deterministic reference provider exists only for CI/E2E and is never represented as a connected account.",
                Style::default().fg(t.muted),
            )),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn activity_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    if let Some(error) = &app.runtime_error {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("{}  Runtime state unavailable", t.glyphs.attention),
                    Style::default().fg(t.danger).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(error.clone()),
            ])
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    if app.runs.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("{}  No durable runs yet", t.glyphs.activity),
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(
                    "The single-agent runtime is installed and its durable catalog is healthy.",
                ),
                Line::from(
                    "Configure a production provider profile before starting real model work.",
                ),
            ])
            .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let mut lines = vec![
        Line::from(Span::styled(
            format!("{}  Durable runs", t.glyphs.activity),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for run in app.runs.iter().take(8) {
        let color = run_color(run, t);
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", t.glyphs.ready), Style::default().fg(color)),
            Span::styled(
                format!("{}  ", short_id(&run.run_id)),
                Style::default().fg(t.text).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<11}  ", run_state(run)),
                Style::default().fg(color),
            ),
            Span::styled(short_text(&run.goal, 46), Style::default().fg(t.muted)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn settings_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    frame.render_widget(
        Paragraph::new(vec![
            kv(
                t.glyphs.settings,
                "interaction",
                "keyboard first",
                t.text,
                t,
            ),
            kv(
                t.glyphs.arrow,
                "navigation",
                "arrows / Enter / Esc / Tab",
                t.text,
                t,
            ),
            kv(t.glyphs.command, "palette", "Ctrl+K", t.accent, t),
            kv(t.glyphs.command, "help", "?", t.accent_alt, t),
            kv(
                t.glyphs.environment,
                "icon fallback",
                "EVERYTHING_ASCII=1",
                t.text,
                t,
            ),
            Line::from(""),
            Line::from(Span::styled(
                "Colors honor NO_COLOR; RGB is used only when truecolor is advertised.",
                Style::default().fg(t.muted),
            )),
        ])
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn footer(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
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
                format!(" {} ", t.glyphs.attention),
                Style::default().fg(t.warning),
            ),
            Span::raw(notice.clone()),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " everything ",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(env!("CARGO_PKG_VERSION"), Style::default().fg(t.muted)),
            Span::styled("   ·   ", Style::default().fg(t.border)),
            Span::styled(hint, Style::default().fg(t.muted)),
        ])
    };
    frame.render_widget(
        Paragraph::new(line)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(t.border)),
            )
            .alignment(Alignment::Left),
        area,
    );
}

fn render_palette(frame: &mut Frame<'_>, app: &AppState) {
    let t = app.theme;
    let area = centered(
        76.min(frame.area().width.saturating_sub(4)),
        18.min(frame.area().height.saturating_sub(2)),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Block::default().style(Style::default().bg(t.background)),
        area,
    );
    let block = card(" COMMAND PALETTE · Ctrl+K ", t, true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let layout = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {}  ", t.glyphs.command),
                Style::default().fg(t.accent_alt),
            ),
            Span::styled("> ", Style::default().fg(t.accent)),
            Span::raw(app.palette_query.clone()),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(t.border)),
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
                    Style::default().fg(t.text).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {}", entry.hint), Style::default().fg(t.muted)),
            ]))
        })
        .collect::<Vec<_>>();
    let selected =
        (!items.is_empty()).then_some(app.palette_index.min(items.len().saturating_sub(1)));
    let mut state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("  › ")
            .highlight_style(Style::default().fg(t.accent)),
        layout[1],
        &mut state,
    );
}

fn render_help(frame: &mut Frame<'_>, t: Theme) {
    let area = centered(
        66.min(frame.area().width.saturating_sub(4)),
        19.min(frame.area().height.saturating_sub(2)),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Keyboard shortcuts",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            kv(t.glyphs.arrow, "arrows", "navigate", t.text, t),
            kv(t.glyphs.command, "Enter", "open / confirm", t.text, t),
            kv(t.glyphs.command, "Esc", "back / close", t.text, t),
            kv(t.glyphs.command, "Tab", "change focus", t.text, t),
            kv(t.glyphs.command, "Ctrl+K", "command palette", t.accent, t),
            kv(t.glyphs.providers, "Ctrl+P", "providers", t.accent_alt, t),
            kv(t.glyphs.activity, "Ctrl+L", "activity", t.accent_alt, t),
            kv(t.glyphs.settings, "Ctrl+,", "settings", t.text, t),
            kv(t.glyphs.command, "?", "help", t.text, t),
            kv(t.glyphs.command, "q", "quit outside text input", t.text, t),
        ])
        .block(card(" EVERYTHING HELP ", t, true))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn card<'a>(title: &'a str, t: Theme, focused: bool) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(if focused { t.accent } else { t.accent_alt })
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { t.accent } else { t.border }))
        .style(Style::default().bg(t.panel).fg(t.text))
}

fn kv(
    icon: &str,
    key: &str,
    value: impl Into<String>,
    value_color: Color,
    t: Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {icon}  "), Style::default().fg(t.accent)),
        Span::styled(format!("{key:<13}"), Style::default().fg(t.muted)),
        Span::styled(value.into(), Style::default().fg(value_color)),
    ])
}

fn surface(
    icon: &str,
    label: &str,
    detail: impl Into<String>,
    status: &str,
    status_color: Color,
    t: Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!(" {icon}  "), Style::default().fg(t.accent)),
        Span::styled(
            format!("{label:<18}"),
            Style::default().fg(t.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{}  ", detail.into()), Style::default().fg(t.muted)),
        Span::styled(
            format!("{} {status}", t.glyphs.ready),
            Style::default().fg(status_color),
        ),
    ])
}

fn runtime_label(app: &AppState) -> String {
    if app.runtime_error.is_some() {
        "state error".to_owned()
    } else if let Some(run) = app.runs.first() {
        format!("{} run(s) · {}", app.runs.len(), run_state(run))
    } else {
        "ready · no runs".to_owned()
    }
}

fn run_state(run: &RunSummary) -> String {
    format!("{:?}", run.state).to_ascii_lowercase()
}

fn run_color(run: &RunSummary, t: Theme) -> Color {
    if run.accepted {
        t.success
    } else if run.state.is_terminal() {
        t.danger
    } else if run.interrupted {
        t.warning
    } else {
        t.accent
    }
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
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

fn short_text(value: &str, max_chars: usize) -> String {
    let mut text = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        text.push('…');
    }
    text
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::render;
    use crate::app::tests::app;

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
