//! Deterministic, provider-independent foundation contracts for AER.
//!
//! This crate intentionally has no third-party dependencies. It owns only
//! semantic identities and invariants that later runtime/adapters must obey.

pub mod compatibility;
pub mod contracts;
pub mod resources;
