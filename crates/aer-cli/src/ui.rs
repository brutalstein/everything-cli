use std::path::Path;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{AppState, FocusTarget, Overlay, Screen},
    slash,
    theme::Theme,
};

const HERO: [&str; 5] = [
    "  ___ _   _____ _ __ _   _| |_| |__ (_)_ __   __ _",
    " / _ \\ | / / _ \\ '__| | | | __| '_ \\| | '_ \\ / _` |",
    "|  __/\\ V /  __/ |  | |_| | |_| | | | | | | | (_| |",
    " \\___| \\_/ \\___|_|   \\__, |\\__|_| |_|_|_| |_|\\__, |",
    "                   |___/                     |___/",
];

pub fn render(frame: &mut Frame<'_>, app: &AppState) {
    let area = frame.area();
    let t = app.theme;
    frame.render_widget(
        Block::default().style(Style::default().bg(t.background).fg(t.text)),
        area,
    );

    let suggestions = app.slash_suggestions();
    let suggestion_height = if suggestions.is_empty() {
        0
    } else {
        u16::try_from(suggestions.len().min(5) + 1).unwrap_or(6)
    };

    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(suggestion_height),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .split(area);

    render_header(frame, root[0], app);
    if app.screen == Screen::Home {
        render_conversation(frame, root[1], app);
    } else {
        render_detail(frame, root[1], app);
    }
    if suggestion_height > 0 {
        render_slash_suggestions(frame, root[2], app);
    }
    render_composer(frame, root[3], app);
    render_statusline(frame, root[4], app);

    if app.overlay == Overlay::Help {
        render_help(frame, area, app);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let workspace = workspace_name(&app.workspace.repo_root);
    let left = Line::from(vec![
        Span::styled(
            " everything ",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled("· ", Style::default().fg(t.border)),
        Span::styled(workspace, Style::default().fg(t.text)),
        Span::styled("  ", Style::default().fg(t.border)),
        Span::styled(
            app.workspace.branch.as_deref().unwrap_or("detached"),
            Style::default().fg(t.muted),
        ),
    ]);

    let surface = if app.screen == Screen::Home {
        "chat".to_owned()
    } else {
        format!("/{}", surface_name(app.screen))
    };
    let right = Line::from(vec![
        Span::styled(surface, Style::default().fg(t.accent_alt)),
        Span::styled("  ·  ", Style::default().fg(t.border)),
        Span::styled(
            if app.workspace.is_clean() {
                "clean"
            } else {
                "dirty"
            },
            Style::default().fg(if app.workspace.is_clean() {
                t.success
            } else {
                t.warning
            }),
        ),
        Span::raw(" "),
    ]);

    let columns =
        Layout::horizontal([Constraint::Percentage(64), Constraint::Percentage(36)]).split(area);
    frame.render_widget(
        Paragraph::new(left).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(t.border)),
        ),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(t.border)),
        ),
        columns[1],
    );
}

fn render_conversation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let messages = app
        .spec
        .as_ref()
        .map(|spec| spec.intent.messages.as_slice())
        .unwrap_or(&[]);

    if messages.is_empty() {
        render_empty_conversation(frame, area, app);
        return;
    }

    let max_messages = if area.height > 28 { 8 } else { 5 };
    let start = messages.len().saturating_sub(max_messages);
    let mut lines = Vec::new();

    for message in &messages[start..] {
        lines.push(Line::from(Span::styled(
            "You",
            Style::default()
                .fg(t.accent_alt)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            message.text.clone(),
            Style::default().fg(t.text),
        )));
        lines.push(Line::from(""));
    }

    if let Some(spec) = app.spec.as_ref() {
        if let Some(question) = spec.next_question() {
            lines.push(Line::from(Span::styled(
                "everything",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "I need one product decision before the contract can become more specific.",
                Style::default().fg(t.muted),
            )));
            lines.push(Line::from(Span::styled(
                question.question.clone(),
                Style::default().fg(t.text),
            )));
            lines.push(Line::from(""));
        } else if spec.ir.is_some() {
            lines.push(Line::from(vec![
                Span::styled(
                    "everything  ",
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "Engineering IR revision {} · semantic checksum {}",
                        spec.revision,
                        if spec.semantic_checksum_clean() {
                            "clean"
                        } else {
                            "needs attention"
                        }
                    ),
                    Style::default().fg(if spec.semantic_checksum_clean() {
                        t.success
                    } else {
                        t.warning
                    }),
                ),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().padding(ratatui::widgets::Padding::new(3, 3, 1, 1))),
        area,
    );
}

fn render_empty_conversation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    if area.width < 70 || area.height < 15 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "everything",
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "Tell me what you want to build.",
                    Style::default().fg(t.text),
                )),
                Line::from(Span::styled(
                    "Type / to discover explicit controls.",
                    Style::default().fg(t.muted),
                )),
            ])
            .alignment(Alignment::Center),
            area,
        );
        return;
    }

    let mut lines = vec![Line::from(""), Line::from("")];
    for (index, row) in HERO.iter().enumerate() {
        lines.push(Line::from(Span::styled(
            *row,
            Style::default()
                .fg(if index < 2 { t.accent } else { t.accent_alt })
                .add_modifier(if index == 0 {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        )));
    }
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled("One CLI for work that spans ", Style::default().fg(t.text)),
            Span::styled(
                "everything.",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Describe the outcome. everything records real intent first and only asks when a decision materially changes the result.",
            Style::default().fg(t.muted),
        )),
        Line::from(vec![
            Span::styled("/", Style::default().fg(t.accent_alt)),
            Span::styled(" commands   ", Style::default().fg(t.muted)),
            Span::styled("/providers", Style::default().fg(t.accent_alt)),
            Span::styled(" setup state   ", Style::default().fg(t.muted)),
            Span::styled("/intent", Style::default().fg(t.accent_alt)),
            Span::styled(" contract state", Style::default().fg(t.muted)),
        ]),
    ]);

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let title = format!(" {} · Esc returns to chat ", surface_title(app.screen));
    let lines = match app.screen {
        Screen::Intent => intent_lines(app),
        Screen::Research => research_lines(app),
        Screen::EngineeringIr => ir_lines(app),
        Screen::Workspace => workspace_lines(app),
        Screen::Environment => environment_lines(app),
        Screen::Providers => provider_lines(app),
        Screen::Activity => activity_lines(app),
        Screen::Settings => settings_lines(app),
        Screen::Home => Vec::new(),
    };
    frame.render_widget(
        Paragraph::new(lines)
            .block(detail_block(&title, t))
            .wrap(Wrap { trim: false }),
        inset(area, 2, 1),
    );
}

fn intent_lines(app: &AppState) -> Vec<Line<'static>> {
    let t = app.theme;
    let Some(spec) = app.spec.as_ref() else {
        return vec![muted("No durable intent has been recorded yet.", t)];
    };
    let mut lines = vec![
        metric("messages", spec.intent.messages.len().to_string(), t),
        metric("goals", spec.intent.goals.len().to_string(), t),
        metric("constraints", spec.intent.constraints.len().to_string(), t),
        metric(
            "acceptance criteria",
            spec.intent.acceptance_criteria.len().to_string(),
            t,
        ),
        metric(
            "user decisions",
            spec.intent.user_decisions.len().to_string(),
            t,
        ),
        metric("open unknowns", spec.open_unknown_count().to_string(), t),
        Line::from(""),
    ];
    if let Some(question) = spec.next_question() {
        lines.push(label("next question", t));
        lines.push(Line::from(Span::styled(
            question.question.clone(),
            Style::default().fg(t.text),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "No unresolved user question is currently required.",
            Style::default().fg(t.success),
        )));
    }
    lines
}

fn research_lines(app: &AppState) -> Vec<Line<'static>> {
    let t = app.theme;
    let Some(spec) = app.spec.as_ref() else {
        return vec![muted("No research state exists for this workspace.", t)];
    };
    let mut lines = vec![
        metric("artifacts", spec.research_artifact_count.to_string(), t),
        Line::from(Span::styled(
            "External research is evidence, never automatic authority.",
            Style::default().fg(t.muted),
        )),
        Line::from(""),
    ];
    let findings = spec
        .ir
        .as_ref()
        .map(|ir| ir.research_findings.as_slice())
        .unwrap_or(&[]);
    if findings.is_empty() {
        lines.push(muted("No source-backed research claims are recorded.", t));
    } else {
        for finding in findings.iter().take(12) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}  ", finding.claim_id),
                    Style::default().fg(t.accent_alt),
                ),
                Span::styled(
                    format!("{:?}", finding.status).to_ascii_lowercase(),
                    Style::default().fg(t.text),
                ),
                Span::styled("  ", Style::default().fg(t.border)),
                Span::styled(finding.statement.clone(), Style::default().fg(t.muted)),
            ]));
        }
    }
    lines
}

fn ir_lines(app: &AppState) -> Vec<Line<'static>> {
    let t = app.theme;
    let Some(spec) = app.spec.as_ref() else {
        return vec![muted("No Engineering IR has been compiled yet.", t)];
    };
    let Some(ir) = spec.ir.as_ref() else {
        return vec![muted("No Engineering IR has been compiled yet.", t)];
    };
    let mut lines = vec![
        metric("revision", spec.revision.to_string(), t),
        metric(
            "semantic checksum",
            if spec.semantic_checksum_clean() {
                "clean".to_owned()
            } else {
                "needs attention".to_owned()
            },
            t,
        ),
        metric("goals", ir.goals.len().to_string(), t),
        metric(
            "requirements",
            ir.functional_requirements.len().to_string(),
            t,
        ),
        metric("constraints", ir.constraints.len().to_string(), t),
        metric(
            "acceptance criteria",
            ir.acceptance_criteria.len().to_string(),
            t,
        ),
        metric("unknowns", ir.unknowns.len().to_string(), t),
        Line::from(""),
    ];
    if let Some(delta) = spec.latest_delta.as_ref() {
        lines.push(label("latest SpecDelta", t));
        lines.push(Line::from(Span::styled(
            format!(
                "{} → {} · +{} changed {} invalidated {}",
                delta.base_revision,
                delta.new_revision,
                delta.added_ids.len(),
                delta.changed_ids.len(),
                delta.invalidated_ids.len()
            ),
            Style::default().fg(t.muted),
        )));
    }
    lines
}

fn workspace_lines(app: &AppState) -> Vec<Line<'static>> {
    let t = app.theme;
    vec![
        metric(
            "repository",
            app.workspace.repo_root.display().to_string(),
            t,
        ),
        metric(
            "branch",
            app.workspace
                .branch
                .clone()
                .unwrap_or_else(|| "detached".to_owned()),
            t,
        ),
        metric("head", short_id(&app.workspace.head_commit), t),
        metric(
            "working tree",
            if app.workspace.is_clean() {
                "clean".to_owned()
            } else {
                "dirty".to_owned()
            },
            t,
        ),
        metric(
            "untracked paths",
            app.workspace.untracked_paths.len().to_string(),
            t,
        ),
    ]
}

fn environment_lines(app: &AppState) -> Vec<Line<'static>> {
    let t = app.theme;
    let mut lines = vec![
        metric("OS", app.environment.os.clone(), t),
        metric("architecture", app.environment.architecture.clone(), t),
        metric("fingerprint", short_id(&app.environment.digest), t),
        Line::from(""),
        label("detected tools", t),
    ];
    if app.environment.tools.is_empty() {
        lines.push(muted("No tools were reported by the environment probe.", t));
    } else {
        for tool in app.environment.tools.iter().take(14) {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<20}", tool.name), Style::default().fg(t.text)),
                Span::styled(
                    tool.version.clone().unwrap_or_else(|| "unknown".to_owned()),
                    Style::default().fg(t.muted),
                ),
            ]));
        }
    }
    lines
}

fn provider_lines(app: &AppState) -> Vec<Line<'static>> {
    let t = app.theme;
    vec![
        Line::from(vec![
            Span::styled(
                format!("{}  gateway", t.glyphs.providers),
                Style::default().fg(t.accent_alt),
            ),
            Span::styled("   ready", Style::default().fg(t.success)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{}  production profile", t.glyphs.attention),
                Style::default().fg(t.warning),
            ),
            Span::styled("   not configured", Style::default().fg(t.warning)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "No fake account form is shown. Provider onboarding becomes interactive only when the supported secure credential transport exists.",
            Style::default().fg(t.muted),
        )),
        Line::from(Span::styled(
            "Raw credentials are not stored by this TUI surface.",
            Style::default().fg(t.success),
        )),
    ]
}

fn activity_lines(app: &AppState) -> Vec<Line<'static>> {
    let t = app.theme;
    if let Some(error) = app.runtime_error.as_deref() {
        return vec![Line::from(Span::styled(
            format!("Runtime catalog error: {error}"),
            Style::default().fg(t.danger),
        ))];
    }
    if app.runs.is_empty() {
        return vec![muted("No durable runs exist for this workspace.", t)];
    }
    app.runs
        .iter()
        .take(14)
        .map(|run| {
            Line::from(vec![
                Span::styled(short_id(&run.run_id), Style::default().fg(t.accent_alt)),
                Span::styled("  ", Style::default().fg(t.border)),
                Span::styled(
                    format!("{:?}", run.state).to_ascii_lowercase(),
                    Style::default().fg(if run.accepted { t.success } else { t.text }),
                ),
                Span::styled("  ", Style::default().fg(t.border)),
                Span::styled(run.goal.clone(), Style::default().fg(t.muted)),
            ])
        })
        .collect()
}

fn settings_lines(app: &AppState) -> Vec<Line<'static>> {
    let t = app.theme;
    vec![
        metric("interaction", "conversation + slash commands".to_owned(), t),
        metric("composer", "persistent bottom prompt".to_owned(), t),
        metric("slash menu", "type / to open".to_owned(), t),
        metric(
            "ASCII fallback",
            if std::env::var_os("EVERYTHING_ASCII").is_some() {
                "enabled".to_owned()
            } else {
                "disabled".to_owned()
            },
            t,
        ),
        Line::from(""),
        muted(
            "Workspace views are command-driven overlays; there is no permanent navigation sidebar.",
            t,
        ),
    ]
}

fn render_slash_suggestions(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let suggestions = app.slash_suggestions();
    let items = suggestions
        .iter()
        .take(5)
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<28}", entry.usage),
                    Style::default().fg(t.accent),
                ),
                Span::styled(entry.description, Style::default().fg(t.muted)),
            ]))
        })
        .collect::<Vec<_>>();
    let selected = app.slash_index.min(items.len().saturating_sub(1));
    let mut state = ListState::default().with_selected((!items.is_empty()).then_some(selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                    .border_style(Style::default().fg(t.border))
                    .style(Style::default().bg(t.panel).fg(t.text)),
            )
            .highlight_symbol("› ")
            .highlight_style(
                Style::default()
                    .fg(t.accent_alt)
                    .add_modifier(Modifier::BOLD),
            ),
        area,
        &mut state,
    );
}

fn render_composer(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let (left, right) = split_at_char(&app.composer, app.composer_cursor);
    let prompt = Line::from(vec![
        Span::styled(
            " ❯ ",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(left, Style::default().fg(t.text)),
        Span::styled(
            "▏",
            Style::default()
                .fg(t.accent_alt)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(right, Style::default().fg(t.text)),
    ]);

    let helper = if let Some(notice) = app.notice.as_deref() {
        Line::from(Span::styled(notice, Style::default().fg(t.warning)))
    } else if let Some(error) = app.spec_error.as_deref() {
        Line::from(Span::styled(
            format!("spec error · {error}"),
            Style::default().fg(t.danger),
        ))
    } else {
        Line::from(vec![
            Span::styled("/", Style::default().fg(t.accent_alt)),
            Span::styled(" commands   ", Style::default().fg(t.muted)),
            Span::styled("↑↓", Style::default().fg(t.accent_alt)),
            Span::styled(" history/select   ", Style::default().fg(t.muted)),
            Span::styled("Esc", Style::default().fg(t.accent_alt)),
            Span::styled(" chat/back   ", Style::default().fg(t.muted)),
            Span::styled("/quit", Style::default().fg(t.accent_alt)),
            Span::styled(" exit", Style::default().fg(t.muted)),
        ])
    };

    frame.render_widget(
        Paragraph::new(vec![prompt, helper])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(if app.focus == FocusTarget::Composer {
                        t.accent
                    } else {
                        t.border
                    }))
                    .style(Style::default().bg(t.panel).fg(t.text)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_statusline(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let spec_revision = app.spec.as_ref().map_or(0, |spec| spec.revision);
    let left = Line::from(vec![
        Span::styled(
            workspace_name(&app.workspace.repo_root),
            Style::default().fg(t.muted),
        ),
        Span::styled(" · ", Style::default().fg(t.border)),
        Span::styled(
            app.workspace.branch.as_deref().unwrap_or("detached"),
            Style::default().fg(t.muted),
        ),
        Span::styled(" · ", Style::default().fg(t.border)),
        Span::styled(
            if app.workspace.is_clean() {
                "clean"
            } else {
                "dirty"
            },
            Style::default().fg(if app.workspace.is_clean() {
                t.success
            } else {
                t.warning
            }),
        ),
    ]);
    let right = Line::from(vec![
        Span::styled(
            format!("IR {spec_revision}"),
            Style::default().fg(t.accent_alt),
        ),
        Span::styled(" · ", Style::default().fg(t.border)),
        Span::styled(
            format!("{} run(s)", app.runs.len()),
            Style::default().fg(t.muted),
        ),
        Span::styled(" · ", Style::default().fg(t.border)),
        Span::styled("provider not configured", Style::default().fg(t.warning)),
        Span::raw(" "),
    ]);
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    frame.render_widget(Paragraph::new(left), columns[0]);
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right),
        columns[1],
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let popup = centered_rect(86, 84, area);
    frame.render_widget(Clear, popup);
    let mut lines = vec![
        Line::from(Span::styled(
            "everything commands",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Type / in the composer and filter by name. Views are temporary; Esc returns to chat.",
            Style::default().fg(t.muted),
        )),
        Line::from(""),
    ];
    for entry in slash::ENTRIES {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<30}", entry.usage),
                Style::default().fg(t.accent_alt),
            ),
            Span::styled(entry.description, Style::default().fg(t.text)),
        ]));
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            "Keyboard: ↑↓ select/history · ←→ edit · Enter submit · Esc back · F1 help",
            Style::default().fg(t.muted),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .block(detail_block(" HELP ", t))
            .wrap(Wrap { trim: true }),
        popup,
    );
}

fn detail_block<'a>(title: &'a str, t: Theme) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.border))
        .style(Style::default().bg(t.panel).fg(t.text))
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1))
}

fn metric(label_text: &str, value: String, t: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label_text:<22}"), Style::default().fg(t.muted)),
        Span::styled(value, Style::default().fg(t.text)),
    ])
}

fn label(text: &str, t: Theme) -> Line<'static> {
    Line::from(Span::styled(
        text.to_owned(),
        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
    ))
}

fn muted(text: &str, t: Theme) -> Line<'static> {
    Line::from(Span::styled(text.to_owned(), Style::default().fg(t.muted)))
}

fn surface_name(screen: Screen) -> &'static str {
    match screen {
        Screen::Home => "chat",
        Screen::Intent => "intent",
        Screen::Research => "research",
        Screen::EngineeringIr => "ir",
        Screen::Workspace => "workspace",
        Screen::Environment => "environment",
        Screen::Providers => "providers",
        Screen::Activity => "activity",
        Screen::Settings => "settings",
    }
}

fn surface_title(screen: Screen) -> &'static str {
    match screen {
        Screen::Home => "CHAT",
        Screen::Intent => "INTENT",
        Screen::Research => "RESEARCH",
        Screen::EngineeringIr => "ENGINEERING IR",
        Screen::Workspace => "WORKSPACE",
        Screen::Environment => "ENVIRONMENT",
        Screen::Providers => "PROVIDERS",
        Screen::Activity => "ACTIVITY",
        Screen::Settings => "SETTINGS",
    }
}

fn workspace_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn short_id(value: &str) -> String {
    value.chars().take(14).collect()
}

fn split_at_char(value: &str, char_index: usize) -> (String, String) {
    let byte = value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index);
    (value[..byte].to_owned(), value[byte..].to_owned())
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    Rect {
        x: area.x.saturating_add(horizontal),
        y: area.y.saturating_add(vertical),
        width: area.width.saturating_sub(horizontal.saturating_mul(2)),
        height: area.height.saturating_sub(vertical.saturating_mul(2)),
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use crate::app::{Screen, tests::app};

    use super::render;

    fn rendered(width: u16, height: u16, screen: Screen) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut app = app();
        app.screen = screen;
        terminal.draw(|frame| render(frame, &app)).expect("draw");
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn workspace_shell_has_prompt_and_no_permanent_sidebar() {
        let symbols = rendered(120, 34, Screen::Home);
        assert!(symbols.contains("everything"));
        assert!(symbols.contains("commands"));
        assert!(!symbols.contains("SURFACES"));
    }

    #[test]
    fn slash_views_render_as_transient_detail_surfaces() {
        let symbols = rendered(100, 30, Screen::Providers);
        assert!(symbols.contains("PROVIDERS"));
        assert!(symbols.contains("Esc returns to chat"));
    }

    #[test]
    fn narrow_terminal_keeps_the_composer_visible() {
        let symbols = rendered(72, 24, Screen::Home);
        assert!(symbols.contains("commands"));
    }
}
