//! Executable contract infrastructure for AER.
//!
//! This crate deliberately contains no model-provider or runtime-state dependency.
//! It turns the architecture's checked-in schemas and cross-object invariants into
//! deterministic validation primitives that later runtime layers can reuse.

pub mod benchmark;
pub mod schema;
pub mod semantic;
pub mod telemetry;
