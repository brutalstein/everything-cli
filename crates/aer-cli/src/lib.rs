//! `everything` — terminal-native product surface.
//!
//! Presentation stays outside domain/runtime authority. Natural text and slash
//! commands become typed application actions, and rendering projects authoritative
//! lower-layer state. The bottom composer is persistent on every TUI surface.

mod app;
mod commands;
mod material_icons;
mod slash;
mod theme;
mod ui;

pub use app::{AppState, FocusTarget, Overlay, Screen, UiAction, normalize_key};
pub use commands::run_cli;
pub use theme::{Glyphs, Theme};
pub use ui::render;
