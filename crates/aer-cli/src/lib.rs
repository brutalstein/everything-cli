//! `everything` — low-overhead terminal CLI over the authoritative AER application boundaries.
//!
//! The interactive path is intentionally a line-oriented shell, not a full-screen TUI. It performs
//! no eager environment/runtime/specification discovery and no per-key rendering. Expensive state
//! inspection happens only when the user explicitly requests the corresponding capability.

mod commands;
mod provider_cli;
mod shell;

pub use commands::run_cli;
