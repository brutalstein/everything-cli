//! `everything` — terminal-native product surface.
//!
//! The interactive root is a two-stage product shell: a local launcher selects a
//! real repository workspace, then the workspace surface becomes a conversation-
//! first terminal client. Presentation remains outside domain/runtime authority.

mod app;
mod commands;
mod entry;
mod launcher;
mod material_icons;
mod slash;
mod theme;
mod ui;

pub use app::{AppState, FocusTarget, Overlay, Screen, UiAction, normalize_key};
pub use entry::run_cli;
pub use theme::{Glyphs, Theme};
pub use ui::render;
