use std::fmt::Write as _;

use aer_domain::compatibility::EVENT_SCHEMA_VERSION;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::ObjectHash;

/// Event payload is either small inline JSON, a durable artifact hash, or empty.
#[derive(Clone, Debug)]
pub enum EventPayload {
    None,
    Inline(Value),
    Artifact(ObjectHash),
}

/// Causation is either another journal event or an explicitly external identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Causation {
    Event(String),
    External(String),
}

/// Material event accepted by the durable journal.
#[derive(Clone, Debug)]
pub struct NewEvent {
    pub project_id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub event_type: String,
    pub schema_version: u32,
    pub payload: EventPayload,
    pub causation: Option<Causation>,
    pub correlation_id: Option<String>,
}

impl NewEvent {
    #[must_use]
    pub fn new(project_id: impl Into<String>, event_type: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            run_id: None,
            task_id: None,
            event_type: event_type.into(),
            schema_version: EVENT_SCHEMA_VERSION,
            payload: EventPayload::None,
            causation: None,
            correlation_id: None,
        }
    }
}

/// Immutable row reconstructed from the append-only event journal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredEvent {
    pub sequence: i64,
    pub event_id: String,
    pub project_id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub event_type: String,
    pub schema_version: u32,
    pub timestamp: String,
    pub payload_json: Option<String>,
    pub payload_artifact_hash: Option<ObjectHash>,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
}

/// Deterministic materialized summary of the immutable event stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalProjection {
    pub project_id: String,
    pub event_count: u64,
    pub last_sequence: Option<i64>,
    pub last_event_id: Option<String>,
    pub rolling_digest: String,
}

impl JournalProjection {
    #[must_use]
    pub(crate) fn empty(project_id: &str) -> Self {
        Self {
            project_id: project_id.to_owned(),
            event_count: 0,
            last_sequence: None,
            last_event_id: None,
            rolling_digest: empty_digest(project_id),
        }
    }

    #[must_use]
    pub(crate) fn advance(&self, event: &StoredEvent) -> Self {
        debug_assert_eq!(self.project_id, event.project_id);
        Self {
            project_id: self.project_id.clone(),
            event_count: self.event_count + 1,
            last_sequence: Some(event.sequence),
            last_event_id: Some(event.event_id.clone()),
            rolling_digest: event_digest(&self.rolling_digest, event),
        }
    }
}

fn empty_digest(project_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"AER_JOURNAL_PROJECTION_EMPTY_V1\0");
    update_field(&mut hasher, project_id.as_bytes());
    digest_hex(hasher)
}

fn event_digest(previous: &str, event: &StoredEvent) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"AER_JOURNAL_PROJECTION_EVENT_V1\0");
    update_field(&mut hasher, previous.as_bytes());
    update_field(&mut hasher, event.sequence.to_string().as_bytes());
    update_field(&mut hasher, event.event_id.as_bytes());
    update_field(&mut hasher, event.project_id.as_bytes());
    update_optional(&mut hasher, event.run_id.as_deref());
    update_optional(&mut hasher, event.task_id.as_deref());
    update_field(&mut hasher, event.event_type.as_bytes());
    update_field(&mut hasher, event.schema_version.to_string().as_bytes());
    update_field(&mut hasher, event.timestamp.as_bytes());
    update_optional(&mut hasher, event.payload_json.as_deref());
    update_optional(
        &mut hasher,
        event.payload_artifact_hash.as_ref().map(ObjectHash::as_str),
    );
    update_optional(&mut hasher, event.causation_id.as_deref());
    update_optional(&mut hasher, event.correlation_id.as_deref());
    digest_hex(hasher)
}

fn update_optional(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            update_field(hasher, value.as_bytes());
        }
        None => hasher.update([0]),
    }
}

fn update_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn digest_hex(hasher: Sha256) -> String {
    let digest = hasher.finalize();
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
