use std::{
    error::Error,
    io::{self, IsTerminal},
    path::Path,
};

use crossterm::event::{self, Event};

use crate::{
    app::{AppState, FocusTarget, Overlay, Screen, UiAction, normalize_key},
    commands,
    launcher::choose_workspace,
    ui::render,
};

pub fn run_cli() -> Result<(), Box<dyn Error>> {
    let interactive_root =
        std::env::args_os().len() == 1 && io::stdin().is_terminal() && io::stdout().is_terminal();

    if !interactive_root {
        return commands::run_cli();
    }

    let cwd = std::env::current_dir()?;
    run_interactive(&cwd)
}

fn run_interactive(start: &Path) -> Result<(), Box<dyn Error>> {
    let Some(workspace_root) = choose_workspace(start)? else {
        return Ok(());
    };
    let mut app = AppState::discover(&workspace_root)?;

    ratatui::run(|terminal| -> io::Result<()> {
        loop {
            terminal.draw(|frame| render(frame, &app))?;
            match event::read()? {
                Event::Key(key) => {
                    if let Some(action) = normalize_key(key, app.overlay) {
                        handle_workspace_action(&mut app, action);
                    }
                }
                Event::Paste(text) => app.insert_text(&text),
                Event::Resize(_, _) | Event::FocusGained | Event::FocusLost | Event::Mouse(_) => {}
            }
            if app.should_quit {
                return Ok(());
            }
        }
    })?;
    Ok(())
}

fn handle_workspace_action(app: &mut AppState, action: UiAction) {
    if matches!(action, UiAction::NextFocus | UiAction::PreviousFocus) {
        app.focus = FocusTarget::Composer;
        return;
    }

    if app.overlay == Overlay::None
        && app.focus == FocusTarget::Composer
        && app.composer.is_empty()
        && action == UiAction::MoveUp
    {
        if let Some((index, value)) = app
            .history
            .len()
            .checked_sub(1)
            .and_then(|index| app.history.get(index).cloned().map(|value| (index, value)))
        {
            app.composer = value;
            app.composer_cursor = app.composer.chars().count();
            app.history_index = Some(index);
        }
        return;
    }

    if app.overlay == Overlay::None
        && app.focus == FocusTarget::Composer
        && app.composer.is_empty()
        && matches!(action, UiAction::MoveDown | UiAction::Confirm)
    {
        return;
    }

    let plain_submission = action == UiAction::Confirm
        && !app.composer.trim().is_empty()
        && !app.composer.trim_start().starts_with('/');

    app.handle(action);

    if plain_submission && app.spec_error.is_none() {
        app.screen = Screen::Home;
        app.nav_index = 0;
        app.focus = FocusTarget::Composer;
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{FocusTarget, UiAction, tests::app};

    use super::handle_workspace_action;

    #[test]
    fn empty_enter_does_not_open_hidden_navigation() {
        let mut app = app();
        handle_workspace_action(&mut app, UiAction::Confirm);
        assert_eq!(app.screen, crate::Screen::Home);
    }

    #[test]
    fn up_arrow_recalls_last_prompt_without_sidebar_navigation() {
        let mut app = app();
        app.history.push("last prompt".to_owned());
        handle_workspace_action(&mut app, UiAction::MoveUp);
        assert_eq!(app.composer, "last prompt");
    }

    #[test]
    fn tab_never_moves_focus_to_a_hidden_navigation_surface() {
        let mut app = app();
        handle_workspace_action(&mut app, UiAction::NextFocus);
        assert_eq!(app.focus, FocusTarget::Composer);
        handle_workspace_action(&mut app, UiAction::PreviousFocus);
        assert_eq!(app.focus, FocusTarget::Composer);
    }
}
