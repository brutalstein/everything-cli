//! `everything` — terminal-native product surface.
//!
//! Presentation stays outside domain/runtime authority. Keyboard input becomes
//! typed UI actions and rendering projects authoritative lower-layer state.

mod app;
mod commands;
mod theme;
mod ui;

pub use app::{AppState, FocusTarget, Overlay, Screen, UiAction, normalize_key};
pub use commands::run_cli;
pub use theme::{Glyphs, Theme};
pub use ui::render;
