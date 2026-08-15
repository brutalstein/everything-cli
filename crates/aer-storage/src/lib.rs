//! Crash-safe local durable-state kernel for AER.
//!
//! This crate owns the initial SQLite event journal, content-addressed object
//! store, migration/preflight boundary, and deterministic journal projection.
//! It deliberately does not own project/run/task state-machine semantics.

mod error;
mod event;
mod journal;
mod migration;
mod object_store;
mod paths;
mod state;

pub use error::{Result, StorageError};
pub use event::{Causation, EventPayload, JournalProjection, NewEvent, StoredEvent};
pub use object_store::{ObjectHash, ObjectMetadata, Sensitivity};
pub use paths::{DurabilityDiagnostics, StoragePaths};
pub use state::DurableState;

#[cfg(test)]
mod tests;
