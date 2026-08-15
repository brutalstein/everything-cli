//! AER application/runtime API root.
//!
//! Phase-1 execution remains isolated in its proven implementation module while
//! Phase-2 specification/research APIs are added beside it. Public callers see
//! one application boundary rather than depending on internal module ownership.

use std::path::{Path, PathBuf};

use aer_exec::lowercase_hex;
use aer_storage::DurableState;
use sha2::{Digest, Sha256};

#[path = "lib.rs"]
mod phase1_runtime;

pub use phase1_runtime::*;
pub mod spec;

fn open_store(project_root: &Path) -> Result<DurableState, aer_storage::StorageError> {
    DurableState::open(project_root.join("durable"))
}

fn project_runtime_root(state_home: &Path, project_id: &str) -> PathBuf {
    let digest = Sha256::digest(project_id.as_bytes());
    state_home
        .join("projects")
        .join(lowercase_hex(digest.as_ref()))
}
