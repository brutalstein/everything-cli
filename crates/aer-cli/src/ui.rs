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

const TAGLINE: &str = "One CLI for work that spans everything.";

pub fn render(frame: &mut Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(app.theme.background).fg(app.theme.text)),
        area,
    );

    let suggestions = app.slash_suggestions();
    let suggestion_height = if suggestions.is_empty() {
        0
    } else {
        u16::try_from(suggestions.len().min(4) + 2).unwrap_or(6)
    };
    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(7),
        Constraint::Length(suggestion_height),
        Constraint::Length(4),
    ])
    .split(area);

    render_header(frame, root[0], app);
    render_body(frame, root[1], app);
    if suggestion_height > 0 {
        render_slash_suggestions(frame, root[2], app);
    }
    render_composer(frame, root[3], app);

    if app.overlay == Overlay::Help {
        render_help(frame, area, app);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let left = Line::from(vec![
        Span::styled(
            "  everything",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  /  ", Style::default().fg(t.border)),
        Span::styled(
            app.screen.label(),
            Style::default().fg(t.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", app.screen.slash()),
            Style::default().fg(t.muted),
        ),
    ]);
    let right = Line::from(vec![
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
        Span::styled("  ·  ", Style::default().fg(t.border)),
        Span::styled(
            app.workspace.branch.as_deref().unwrap_or("detached"),
            Style::default().fg(t.muted),
        ),
        Span::raw("  "),
    ]);
    let columns =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).split(area);
    frame.render_widget(Paragraph::new(left), columns[0]);
    frame.render_widget(
        Paragraph::new(right).alignment(Alignment::Right),
        columns[1],
    );
}

fn render_body(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    if area.width >= 86 {
        let columns = Layout::horizontal([Constraint::Length(27), Constraint::Min(20)]).split(area);
        render_navigation(frame, columns[0], app);
        render_content(frame, columns[1], app);
    } else {
        let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(4)]).split(area);
        render_compact_navigation(frame, rows[0], app);
        render_content(frame, rows[1], app);
    }
}

fn render_navigation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let items = Screen::ALL
        .iter()
        .map(|screen| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", screen.icon(&t.glyphs)),
                    Style::default().fg(t.accent_alt),
                ),
                Span::styled(screen.label(), Style::default().fg(t.text)),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected(Some(app.nav_index));
    let focused = app.focus == FocusTarget::Navigation;
    frame.render_stateful_widget(
        List::new(items)
            .block(card(" SURFACES ", t, focused))
            .highlight_symbol("› ")
            .highlight_style(Style::default().fg(t.accent).add_modifier(Modifier::BOLD)),
        area,
        &mut state,
    );
}

fn render_compact_navigation(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let spans = Screen::ALL
        .iter()
        .enumerate()
        .flat_map(|(index, screen)| {
            let selected = index == app.nav_index;
            [
                Span::styled(
                    format!("{} {}", screen.icon(&t.glyphs), screen.label()),
                    Style::default()
                        .fg(if selected { t.accent } else { t.muted })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled("   ", Style::default().fg(t.border)),
            ]
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .block(card(" SURFACES ", t, app.focus == FocusTarget::Navigation))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_content(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    match app.screen {
        Screen::Home => render_home(frame, area, app),
        Screen::Intent => render_intent(frame, area, app),
        Screen::Research => render_research(frame, area, app),
        Screen::EngineeringIr => render_ir(frame, area, app),
        Screen::Workspace => render_workspace(frame, area, app),
        Screen::Environment => render_environment(frame, area, app),
        Screen::Providers => render_providers(frame, area, app),
        Screen::Activity => render_activity(frame, area, app),
        Screen::Settings => render_settings(frame, area, app),
    }
}

fn render_home(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    if area.width >= 76 && area.height >= 18 {
        let rows = Layout::vertical([Constraint::Length(7), Constraint::Min(8)]).split(area);
        let hero = vec![
            Line::from(""),
            Line::from(Span::styled(
                "everything",
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(TAGLINE, Style::default().fg(t.muted))),
            Line::from(""),
            Line::from(vec![
                Span::styled("Type naturally", Style::default().fg(t.text)),
                Span::styled("  or  ", Style::default().fg(t.muted)),
                Span::styled("/help", Style::default().fg(t.accent_alt)),
                Span::styled(" for deterministic actions", Style::default().fg(t.muted)),
            ]),
        ];
        frame.render_widget(
            Paragraph::new(hero)
                .alignment(Alignment::Center)
                .block(card(" PRODUCT ", t, app.focus == FocusTarget::Content)),
            rows[0],
        );
        let columns = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[1]);
        render_home_state(frame, columns[0], app);
        render_home_next(frame, columns[1], app);
    } else {
        render_home_state(frame, area, app);
    }
}

fn render_home_state(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let spec = app.spec.as_ref();
    let lines = vec![
        kv(
            t.glyphs.workspace,
            "workspace",
            workspace_name(&app.workspace.repo_root),
            if app.workspace.is_clean() {
                t.success
            } else {
                t.warning
            },
            t,
        ),
        kv(
            t.glyphs.intent,
            "intent",
            spec.map_or("none".to_owned(), |spec| {
                format!(
                    "{} message(s) · {} unknown(s)",
                    spec.intent.messages.len(),
                    spec.open_unknown_count()
                )
            }),
            if app.spec_error.is_some() {
                t.danger
            } else {
                t.accent
            },
            t,
        ),
        kv(
            t.glyphs.engineering_ir,
            "IR",
            spec.and_then(|spec| spec.ir.as_ref())
                .map_or("not compiled".to_owned(), |_| {
                    format!("revision {}", spec.map_or(0, |spec| spec.revision))
                }),
            if spec.and_then(|spec| spec.ir.as_ref()).is_some() {
                t.success
            } else {
                t.muted
            },
            t,
        ),
        kv(
            t.glyphs.research,
            "research",
            spec.map_or("0 artifact(s)".to_owned(), |spec| {
                format!("{} artifact(s)", spec.research_artifact_count)
            }),
            t.accent_alt,
            t,
        ),
        kv(
            t.glyphs.activity,
            "runtime",
            if let Some(error) = app.runtime_error.as_deref() {
                format!("error · {error}")
            } else {
                format!("ready · {} run(s)", app.runs.len())
            },
            if app.runtime_error.is_some() {
                t.danger
            } else {
                t.success
            },
            t,
        ),
        kv(
            t.glyphs.providers,
            "provider",
            "gateway ready · profile not configured".to_owned(),
            t.warning,
            t,
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " AUTHORITATIVE STATE ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_home_next(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let (command, title, detail) = if app.runs.iter().any(|run| !run.state.is_terminal()) {
        (
            "/activity",
            "Resume durable work",
            "A non-terminal run exists in authoritative runtime state.",
        )
    } else if let Some(question) = app.spec.as_ref().and_then(|spec| spec.next_question()) {
        (
            "/intent",
            "Resolve the highest-value unknown",
            question.question.as_str(),
        )
    } else if app
        .spec
        .as_ref()
        .and_then(|spec| spec.ir.as_ref())
        .is_none()
    {
        (
            "<type a request>",
            "Start from intent",
            "Natural text is preserved as user-origin intent; unavailable model extraction is never fabricated.",
        )
    } else {
        (
            "/providers",
            "Connect a production provider",
            "The gateway exists, but no authenticated production profile is configured yet.",
        )
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                title,
                Style::default().fg(t.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(detail, Style::default().fg(t.muted))),
            Line::from(""),
            Line::from(vec![
                Span::styled(t.glyphs.arrow, Style::default().fg(t.accent_alt)),
                Span::raw("  "),
                Span::styled(
                    command,
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                ),
            ]),
        ])
        .block(card(" NEXT ACTION ", t, false))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_intent(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let Some(spec) = app.spec.as_ref() else {
        render_empty(frame, area, t, " INTENT ", app.spec_error.as_deref().unwrap_or("No intent is recorded. Type a normal request, or use /goal, /constraint, /accept and /decision."));
        return;
    };
    let mut lines = vec![
        kv(
            t.glyphs.intent,
            "messages",
            spec.intent.messages.len().to_string(),
            t.accent,
            t,
        ),
        kv(
            t.glyphs.ready,
            "goals",
            spec.intent.goals.len().to_string(),
            t.success,
            t,
        ),
        kv(
            t.glyphs.shield,
            "constraints",
            spec.intent.constraints.len().to_string(),
            t.accent_alt,
            t,
        ),
        kv(
            t.glyphs.ready,
            "acceptance",
            spec.intent.acceptance_criteria.len().to_string(),
            t.success,
            t,
        ),
        kv(
            t.glyphs.attention,
            "unknowns",
            spec.open_unknown_count().to_string(),
            if spec.open_unknown_count() == 0 {
                t.success
            } else {
                t.warning
            },
            t,
        ),
        kv(
            t.glyphs.branch,
            "decisions",
            spec.intent.user_decisions.len().to_string(),
            t.accent_alt,
            t,
        ),
        Line::from(""),
    ];
    if let Some(question) = spec.next_question() {
        lines.push(Line::from(Span::styled(
            "Highest-value question",
            Style::default().fg(t.warning).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            question.question.clone(),
            Style::default().fg(t.text),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "question value: {} · resolution: ask_user",
                question.question_value()
            ),
            Style::default().fg(t.muted),
        )));
        lines.push(Line::from(""));
    }
    for goal in spec.intent.goals.iter().take(4) {
        lines.push(Line::from(vec![
            Span::styled("GOAL  ", Style::default().fg(t.accent)),
            Span::styled(goal.statement.clone(), Style::default().fg(t.text)),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " INTENT / DECISIONS / UNKNOWNS ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_research(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let Some(spec) = app.spec.as_ref() else {
        render_empty(
            frame,
            area,
            t,
            " RESEARCH ",
            "No research state exists. /research-import <artifact.json> ingests a real ResearchArtifact; acquisition is not mocked.",
        );
        return;
    };
    let findings = spec
        .ir
        .as_ref()
        .map(|ir| ir.research_findings.as_slice())
        .unwrap_or(&[]);
    let mut lines = vec![
        kv(
            t.glyphs.research,
            "artifacts",
            spec.research_artifact_count.to_string(),
            t.accent_alt,
            t,
        ),
        kv(
            t.glyphs.ready,
            "claims",
            findings.len().to_string(),
            t.accent,
            t,
        ),
        Line::from(Span::styled(
            "External evidence never self-promotes into accepted requirements or decisions.",
            Style::default().fg(t.muted),
        )),
        Line::from(""),
    ];
    if findings.is_empty() {
        lines.push(Line::from(Span::styled(
            "No source-backed research artifact is recorded.",
            Style::default().fg(t.text),
        )));
        lines.push(Line::from(Span::styled(
            "Use /research-import <artifact.json> for schema-validated local ingestion.",
            Style::default().fg(t.accent),
        )));
        lines.push(Line::from(Span::styled(
            "A network/search acquisition adapter is not fabricated in this step.",
            Style::default().fg(t.muted),
        )));
    } else {
        for finding in findings.iter().take(8) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}  ", finding.claim_id),
                    Style::default().fg(t.accent_alt),
                ),
                Span::styled(
                    format!("{:?}", finding.status).to_ascii_lowercase(),
                    Style::default().fg(t.muted),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(finding.statement.clone(), Style::default().fg(t.text)),
            ]));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " RESEARCH EVIDENCE ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_ir(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let Some(spec) = app.spec.as_ref() else {
        render_empty(
            frame,
            area,
            t,
            " ENGINEERING IR ",
            "No Engineering IR exists. Type a request first.",
        );
        return;
    };
    let Some(ir) = spec.ir.as_ref() else {
        render_empty(
            frame,
            area,
            t,
            " ENGINEERING IR ",
            "Intent exists but no Engineering IR was compiled.",
        );
        return;
    };
    let checksum = spec
        .checksum
        .as_ref()
        .map_or("none".to_owned(), |checksum| {
            format!("{:?}", checksum.severity).to_ascii_lowercase()
        });
    let mut lines = vec![
        kv(
            t.glyphs.engineering_ir,
            "revision",
            spec.revision.to_string(),
            t.accent,
            t,
        ),
        kv(
            t.glyphs.shield,
            "semantic checksum",
            checksum,
            if spec.semantic_checksum_clean() {
                t.success
            } else {
                t.warning
            },
            t,
        ),
        kv(
            t.glyphs.ready,
            "goals",
            ir.goals.len().to_string(),
            t.success,
            t,
        ),
        kv(
            t.glyphs.ready,
            "requirements",
            ir.functional_requirements.len().to_string(),
            t.accent,
            t,
        ),
        kv(
            t.glyphs.ready,
            "acceptance criteria",
            ir.acceptance_criteria.len().to_string(),
            t.success,
            t,
        ),
        kv(
            t.glyphs.attention,
            "unknowns",
            ir.unknowns.len().to_string(),
            if ir.unknowns.is_empty() {
                t.success
            } else {
                t.warning
            },
            t,
        ),
        kv(
            t.glyphs.research,
            "research findings",
            ir.research_findings.len().to_string(),
            t.accent_alt,
            t,
        ),
        Line::from(""),
    ];
    if let Some(delta) = spec.latest_delta.as_ref() {
        lines.push(Line::from(Span::styled(
            format!("SpecDelta {} → {}", delta.base_revision, delta.new_revision),
            Style::default()
                .fg(t.accent_alt)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "added={} changed={} invalidated={}",
                delta.added_ids.len(),
                delta.changed_ids.len(),
                delta.invalidated_ids.len()
            ),
            Style::default().fg(t.muted),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " VERSIONED ENGINEERING IR ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_workspace(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let lines = vec![
        kv(
            t.glyphs.workspace,
            "repo",
            app.workspace.repo_root.display().to_string(),
            t.accent,
            t,
        ),
        kv(
            t.glyphs.branch,
            "repo id",
            short_id(&app.workspace.repo_id),
            t.accent_alt,
            t,
        ),
        kv(
            t.glyphs.branch,
            "HEAD",
            short_id(&app.workspace.head_commit),
            t.text,
            t,
        ),
        kv(
            t.glyphs.branch,
            "branch",
            app.workspace
                .branch
                .clone()
                .unwrap_or_else(|| "detached".to_owned()),
            t.text,
            t,
        ),
        kv(
            t.glyphs.shield,
            "state",
            if app.workspace.is_clean() {
                "clean".to_owned()
            } else {
                "dirty".to_owned()
            },
            if app.workspace.is_clean() {
                t.success
            } else {
                t.warning
            },
            t,
        ),
        kv(
            t.glyphs.attention,
            "untracked",
            app.workspace.untracked_paths.len().to_string(),
            if app.workspace.untracked_paths.is_empty() {
                t.success
            } else {
                t.warning
            },
            t,
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " WORKSPACE EVIDENCE ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_environment(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let mut lines = vec![
        kv(
            t.glyphs.environment,
            "platform",
            format!("{} / {}", app.environment.os, app.environment.architecture),
            t.accent,
            t,
        ),
        kv(
            t.glyphs.shield,
            "fingerprint",
            short_id(&app.environment.digest),
            t.accent_alt,
            t,
        ),
        kv(
            t.glyphs.ready,
            "tools",
            app.environment.tools.len().to_string(),
            t.success,
            t,
        ),
        kv(
            t.glyphs.ready,
            "lockfiles",
            app.environment.lockfiles.len().to_string(),
            t.success,
            t,
        ),
        Line::from(""),
    ];
    for tool in app.environment.tools.iter().take(8) {
        lines.push(Line::from(vec![
            Span::styled(format!("{}  ", tool.name), Style::default().fg(t.text)),
            Span::styled(
                tool.version.clone().unwrap_or_else(|| "unknown".to_owned()),
                Style::default().fg(t.muted),
            ),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " ENVIRONMENT IDENTITY ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_providers(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let lines = vec![
        kv(
            t.glyphs.providers,
            "gateway",
            "ready".to_owned(),
            t.success,
            t,
        ),
        kv(
            t.glyphs.attention,
            "production profile",
            "not configured".to_owned(),
            t.warning,
            t,
        ),
        kv(
            t.glyphs.shield,
            "raw credentials",
            "not stored by this surface".to_owned(),
            t.success,
            t,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "This page is real provider state, not a mock configuration form.",
            Style::default().fg(t.text),
        )),
        Line::from(Span::styled(
            "Authenticated production onboarding is enabled only when a supported provider transport/secure credential adapter exists.",
            Style::default().fg(t.muted),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Navigate: ", Style::default().fg(t.muted)),
            Span::styled("/providers", Style::default().fg(t.accent)),
            Span::styled("   Runtime: ", Style::default().fg(t.muted)),
            Span::styled("/activity", Style::default().fg(t.accent_alt)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " PROVIDER GATEWAY ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_activity(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    if let Some(error) = app.runtime_error.as_deref() {
        render_empty(
            frame,
            area,
            t,
            " ACTIVITY ",
            &format!("Runtime catalog error: {error}"),
        );
        return;
    }
    let mut lines = vec![
        kv(
            t.glyphs.activity,
            "durable runs",
            app.runs.len().to_string(),
            t.accent,
            t,
        ),
        Line::from(""),
    ];
    if app.runs.is_empty() {
        lines.push(Line::from(Span::styled(
            "No durable runs are recorded for this workspace.",
            Style::default().fg(t.text),
        )));
    } else {
        for run in app.runs.iter().take(10) {
            let state = format!("{:?}", run.state).to_ascii_lowercase();
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}  ", short_id(&run.run_id)),
                    Style::default().fg(t.accent_alt),
                ),
                Span::styled(
                    format!("{state:<11}"),
                    Style::default().fg(if run.accepted { t.success } else { t.text }),
                ),
                Span::styled(
                    if run.interrupted {
                        " interrupted  "
                    } else {
                        "              "
                    },
                    Style::default().fg(t.warning),
                ),
                Span::styled(run.goal.clone(), Style::default().fg(t.muted)),
            ]));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " DURABLE RUNTIME ACTIVITY ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_settings(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let lines = vec![
        kv(
            t.glyphs.settings,
            "activation model",
            "slash commands + natural text".to_owned(),
            t.accent,
            t,
        ),
        kv(
            t.glyphs.command,
            "composer",
            "persistent bottom input".to_owned(),
            t.success,
            t,
        ),
        kv(
            t.glyphs.command,
            "arrows",
            "navigation / history / slash selection".to_owned(),
            t.success,
            t,
        ),
        kv(
            t.glyphs.command,
            "tab",
            "composer → navigation → content".to_owned(),
            t.text,
            t,
        ),
        kv(
            t.glyphs.command,
            "Esc / Ctrl+C",
            "clear composer or go back".to_owned(),
            t.text,
            t,
        ),
        kv(t.glyphs.command, "F1 / /help", "help".to_owned(), t.text, t),
        kv(
            t.glyphs.settings,
            "ASCII fallback",
            if std::env::var_os("EVERYTHING_ASCII").is_some() {
                "enabled".to_owned()
            } else {
                "disabled".to_owned()
            },
            t.muted,
            t,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "Single-letter q is ordinary text. Exit is /quit.",
            Style::default().fg(t.accent_alt),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(
                " TERMINAL SETTINGS ",
                t,
                app.focus == FocusTarget::Content,
            ))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_slash_suggestions(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let suggestions = app.slash_suggestions();
    let items = suggestions
        .iter()
        .take(4)
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<24}", entry.usage),
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
            .block(card(
                " SLASH COMMANDS · ↑↓ SELECT · ENTER COMPLETE/RUN ",
                t,
                true,
            ))
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
    let message = Line::from(vec![
        Span::styled(
            " › ",
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
    let status = if let Some(notice) = app.notice.as_deref() {
        Line::from(Span::styled(notice, Style::default().fg(t.warning)))
    } else if let Some(error) = app.spec_error.as_deref() {
        Line::from(Span::styled(
            format!("spec error · {error}"),
            Style::default().fg(t.danger),
        ))
    } else {
        Line::from(vec![
            Span::styled(" natural request", Style::default().fg(t.muted)),
            Span::styled("   /help", Style::default().fg(t.accent)),
            Span::styled(" commands", Style::default().fg(t.muted)),
            Span::styled("   ↑↓", Style::default().fg(t.accent_alt)),
            Span::styled(" navigate/history", Style::default().fg(t.muted)),
            Span::styled("   /quit", Style::default().fg(t.accent)),
            Span::styled(" exit", Style::default().fg(t.muted)),
        ])
    };
    frame.render_widget(
        Paragraph::new(vec![message, status])
            .block(card(
                " MESSAGE / SLASH COMMAND ",
                t,
                app.focus == FocusTarget::Composer,
            ))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
    let t = app.theme;
    let popup = centered_rect(86, 84, area);
    frame.render_widget(Clear, popup);
    let mut lines = vec![
        Line::from(Span::styled(
            "everything command model",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Slash commands are the primary activation surface; arrows and text entry remain first-class.",
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
        Line::from(Span::styled("Keyboard: arrows navigate/select · Enter complete/run · Tab changes focus · Esc/Ctrl+C clears/back · F1 closes help", Style::default().fg(t.muted))),
        Line::from(Span::styled("Press Esc, Enter or F1 to close.", Style::default().fg(t.accent))),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(" HELP ", t, true))
            .wrap(Wrap { trim: true }),
        popup,
    );
}

fn render_empty(frame: &mut Frame<'_>, area: Rect, t: Theme, title: &str, message: &str) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(message, Style::default().fg(t.muted))),
        ])
        .block(card(title, t, false))
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn card<'a>(title: &'a str, t: Theme, focused: bool) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { t.accent } else { t.border }))
        .style(Style::default().bg(t.panel).fg(t.text))
}

fn kv(
    icon: &str,
    label: &str,
    value: String,
    value_color: ratatui::style::Color,
    t: Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {icon}  {label:<18}"),
            Style::default().fg(t.muted),
        ),
        Span::styled(value, Style::default().fg(value_color)),
    ])
}

fn split_at_char(value: &str, char_index: usize) -> (String, String) {
    let byte = value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index);
    (value[..byte].to_owned(), value[byte..].to_owned())
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

fn workspace_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn short_id(value: &str) -> String {
    value.chars().take(14).collect()
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use crate::app::tests::app;

    use super::render;

    fn render_size(width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let app = app();
        terminal.draw(|frame| render(frame, &app)).expect("draw");
        let symbols = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(symbols.contains("MESSAGE / SLASH COMMAND"));
    }

    #[test]
    fn persistent_composer_renders_on_wide_terminal() {
        render_size(132, 38);
    }

    #[test]
    fn persistent_composer_renders_on_standard_terminal() {
        render_size(100, 30);
    }

    #[test]
    fn persistent_composer_renders_on_narrow_terminal() {
        render_size(52, 20);
    }
}
