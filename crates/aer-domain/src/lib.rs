//! Deterministic, provider-independent foundation contracts for AER.
//!
//! This crate intentionally has no third-party dependencies. It owns semantic
//! identities, lifecycle rules, and resource-safety invariants that runtime
//! adapters must obey.

pub mod bounded_queue;
pub mod cancellation;
pub mod compatibility;
pub mod contracts;
pub mod leases;
pub mod resource_governor;
pub mod resources;
pub mod runtime_safety;
pub mod state_machines;
