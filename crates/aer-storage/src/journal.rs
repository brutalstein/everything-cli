use aer_domain::compatibility::EVENT_SCHEMA_VERSION;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    Causation, EventPayload, JournalProjection, NewEvent, ObjectHash, Result, StorageError,
    StoredEvent,
    state::{DurableState, validate_identifier},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppendFault {
    None,
    AfterEventInsert,
}

impl DurableState {
    pub fn append_event(&mut self, event: NewEvent) -> Result<StoredEvent> {
        self.append_event_internal(event, AppendFault::None)
    }

    fn append_event_internal(
        &mut self,
        event: NewEvent,
        fault: AppendFault,
    ) -> Result<StoredEvent> {
        validate_event(&event)?;
        if event.schema_version != EVENT_SCHEMA_VERSION {
            return Err(StorageError::UnsupportedEventSchemaVersion {
                found: event.schema_version,
                supported: EVENT_SCHEMA_VERSION,
            });
        }

        let event_id = self
            .event_ids
            .generate()
            .map_err(|source| StorageError::EventIdGeneration(source.to_string()))?
            .to_string();
        let (payload_json, payload_artifact_hash) = encode_payload(&event.payload)?;
        let causation_id = encode_causation(event.causation.as_ref());

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_causation(&transaction, &event.project_id, event.causation.as_ref())?;
        if let Some(hash) = payload_artifact_hash.as_ref() {
            require_artifact_scope(&transaction, &event.project_id, hash)?;
        }
        transaction.execute(
            "INSERT INTO events(
                event_id, project_id, run_id, task_id, event_type, schema_version,
                payload_json, payload_artifact_hash, causation_id, correlation_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                event_id,
                event.project_id,
                event.run_id,
                event.task_id,
                event.event_type,
                i64::from(event.schema_version),
                payload_json,
                payload_artifact_hash.as_ref().map(ObjectHash::as_str),
                causation_id,
                event.correlation_id,
            ],
        )?;
        let sequence = transaction.last_insert_rowid();
        let stored = load_event_by_sequence(&transaction, sequence)?;
        if fault == AppendFault::AfterEventInsert {
            return Err(StorageError::FaultInjected("event_after_insert"));
        }

        let previous = load_projection(&transaction, &stored.project_id)?
            .unwrap_or_else(|| JournalProjection::empty(&stored.project_id));
        let next = previous.advance(&stored);
        write_projection(&transaction, &next)?;
        transaction.commit()?;
        Ok(stored)
    }

    pub fn events(&self, project_id: &str) -> Result<Vec<StoredEvent>> {
        validate_identifier("project_id", project_id)?;
        let mut statement = self.connection.prepare(
            "SELECT sequence, event_id, project_id, run_id, task_id, event_type,
                    schema_version, timestamp, payload_json, payload_artifact_hash,
                    causation_id, correlation_id
             FROM events WHERE project_id = ?1 ORDER BY sequence",
        )?;
        let rows = statement.query_map([project_id], event_row)?;
        let mut events = Vec::new();
        for row in rows {
            events.push(convert_event_row(row?)?);
        }
        Ok(events)
    }

    pub fn replay_projection(&self, project_id: &str) -> Result<JournalProjection> {
        let mut projection = JournalProjection::empty(project_id);
        for event in self.events(project_id)? {
            projection = projection.advance(&event);
        }
        Ok(projection)
    }

    pub fn materialized_projection(&self, project_id: &str) -> Result<JournalProjection> {
        validate_identifier("project_id", project_id)?;
        Ok(load_projection(&self.connection, project_id)?
            .unwrap_or_else(|| JournalProjection::empty(project_id)))
    }

    pub fn verify_projection(&self, project_id: &str) -> Result<JournalProjection> {
        let replayed = self.replay_projection(project_id)?;
        let materialized = self.materialized_projection(project_id)?;
        if replayed != materialized {
            return Err(StorageError::ProjectionMismatch {
                project_id: project_id.to_owned(),
            });
        }
        Ok(replayed)
    }

    pub fn rebuild_projection(&mut self, project_id: &str) -> Result<JournalProjection> {
        let replayed = self.replay_projection(project_id)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if replayed.event_count == 0 {
            transaction.execute(
                "DELETE FROM projection_heads WHERE project_id = ?1",
                [project_id],
            )?;
        } else {
            write_projection(&transaction, &replayed)?;
        }
        transaction.commit()?;
        Ok(replayed)
    }

    /// Verifies replay equivalence and every artifact referenced by this project.
    pub fn verify_project_integrity(&self, project_id: &str) -> Result<JournalProjection> {
        let projection = self.verify_projection(project_id)?;
        for event in self.events(project_id)? {
            if let Some(hash) = event.payload_artifact_hash {
                self.read_object(project_id, &hash)?;
            }
        }
        Ok(projection)
    }

    #[cfg(test)]
    pub(crate) fn append_event_with_transaction_fault(
        &mut self,
        event: NewEvent,
    ) -> Result<StoredEvent> {
        self.append_event_internal(event, AppendFault::AfterEventInsert)
    }
}

fn write_projection(transaction: &Transaction<'_>, projection: &JournalProjection) -> Result<()> {
    transaction.execute(
        "INSERT INTO projection_heads(
            project_id, event_count, last_sequence, last_event_id, rolling_digest
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(project_id) DO UPDATE SET
            event_count = excluded.event_count,
            last_sequence = excluded.last_sequence,
            last_event_id = excluded.last_event_id,
            rolling_digest = excluded.rolling_digest",
        params![
            projection.project_id,
            projection.event_count as i64,
            projection.last_sequence,
            projection.last_event_id,
            projection.rolling_digest,
        ],
    )?;
    Ok(())
}

fn validate_event(event: &NewEvent) -> Result<()> {
    validate_identifier("project_id", &event.project_id)?;
    validate_identifier("event_type", &event.event_type)?;
    for (field, value) in [
        ("run_id", event.run_id.as_deref()),
        ("task_id", event.task_id.as_deref()),
        ("correlation_id", event.correlation_id.as_deref()),
    ] {
        if let Some(value) = value {
            validate_identifier(field, value)?;
        }
    }
    if let Some(causation) = event.causation.as_ref() {
        let value = match causation {
            Causation::Event(value) | Causation::External(value) => value,
        };
        validate_identifier("causation_id", value)?;
    }
    Ok(())
}

fn encode_payload(payload: &EventPayload) -> Result<(Option<String>, Option<ObjectHash>)> {
    match payload {
        EventPayload::None => Ok((None, None)),
        EventPayload::Inline(value) => {
            let value = serde_json::to_string(value).map_err(|source| {
                StorageError::MigrationInvariant(format!(
                    "inline event payload could not serialize: {source}"
                ))
            })?;
            Ok((Some(value), None))
        }
        EventPayload::Artifact(hash) => Ok((None, Some(hash.clone()))),
    }
}

fn encode_causation(causation: Option<&Causation>) -> Option<String> {
    causation.map(|causation| match causation {
        Causation::Event(event_id) => event_id.clone(),
        Causation::External(external_id) => format!("external:{external_id}"),
    })
}

fn validate_causation(
    transaction: &Transaction<'_>,
    project_id: &str,
    causation: Option<&Causation>,
) -> Result<()> {
    let Some(Causation::Event(event_id)) = causation else {
        return Ok(());
    };
    let cause_project: Option<String> = transaction
        .query_row(
            "SELECT project_id FROM events WHERE event_id = ?1",
            [event_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(cause_project) = cause_project else {
        return Err(StorageError::DanglingCausation(event_id.clone()));
    };
    if cause_project != project_id {
        return Err(StorageError::CausationScopeMismatch {
            event_id: event_id.clone(),
            cause_project,
            event_project: project_id.to_owned(),
        });
    }
    Ok(())
}

fn require_artifact_scope(
    transaction: &Transaction<'_>,
    project_id: &str,
    hash: &ObjectHash,
) -> Result<()> {
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM object_scopes WHERE hash = ?1 AND project_id = ?2
         )",
        params![hash.as_str(), project_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(StorageError::ArtifactNotScoped {
            hash: hash.to_string(),
            project_id: project_id.to_owned(),
        })
    }
}

#[derive(Debug)]
struct EventRow {
    sequence: i64,
    event_id: String,
    project_id: String,
    run_id: Option<String>,
    task_id: Option<String>,
    event_type: String,
    schema_version: i64,
    timestamp: String,
    payload_json: Option<String>,
    payload_artifact_hash: Option<String>,
    causation_id: Option<String>,
    correlation_id: Option<String>,
}

fn event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventRow> {
    Ok(EventRow {
        sequence: row.get(0)?,
        event_id: row.get(1)?,
        project_id: row.get(2)?,
        run_id: row.get(3)?,
        task_id: row.get(4)?,
        event_type: row.get(5)?,
        schema_version: row.get(6)?,
        timestamp: row.get(7)?,
        payload_json: row.get(8)?,
        payload_artifact_hash: row.get(9)?,
        causation_id: row.get(10)?,
        correlation_id: row.get(11)?,
    })
}

fn convert_event_row(row: EventRow) -> Result<StoredEvent> {
    let schema_version = u32::try_from(row.schema_version).map_err(|_| {
        StorageError::MigrationInvariant(format!(
            "stored event has invalid schema version {}",
            row.schema_version
        ))
    })?;
    let payload_artifact_hash = row
        .payload_artifact_hash
        .map(ObjectHash::parse)
        .transpose()?;
    Ok(StoredEvent {
        sequence: row.sequence,
        event_id: row.event_id,
        project_id: row.project_id,
        run_id: row.run_id,
        task_id: row.task_id,
        event_type: row.event_type,
        schema_version,
        timestamp: row.timestamp,
        payload_json: row.payload_json,
        payload_artifact_hash,
        causation_id: row.causation_id,
        correlation_id: row.correlation_id,
    })
}

fn load_event_by_sequence(transaction: &Transaction<'_>, sequence: i64) -> Result<StoredEvent> {
    let row = transaction.query_row(
        "SELECT sequence, event_id, project_id, run_id, task_id, event_type,
                schema_version, timestamp, payload_json, payload_artifact_hash,
                causation_id, correlation_id
         FROM events WHERE sequence = ?1",
        [sequence],
        event_row,
    )?;
    convert_event_row(row)
}

trait ProjectionConnection {
    fn projection_row(
        &self,
        project_id: &str,
    ) -> rusqlite::Result<Option<(i64, i64, String, String)>>;
}

impl ProjectionConnection for Connection {
    fn projection_row(
        &self,
        project_id: &str,
    ) -> rusqlite::Result<Option<(i64, i64, String, String)>> {
        self.query_row(
            "SELECT event_count, last_sequence, last_event_id, rolling_digest
             FROM projection_heads WHERE project_id = ?1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
    }
}

impl ProjectionConnection for Transaction<'_> {
    fn projection_row(
        &self,
        project_id: &str,
    ) -> rusqlite::Result<Option<(i64, i64, String, String)>> {
        self.query_row(
            "SELECT event_count, last_sequence, last_event_id, rolling_digest
             FROM projection_heads WHERE project_id = ?1",
            [project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
    }
}

fn load_projection(
    connection: &impl ProjectionConnection,
    project_id: &str,
) -> Result<Option<JournalProjection>> {
    let Some((event_count, last_sequence, last_event_id, rolling_digest)) =
        connection.projection_row(project_id)?
    else {
        return Ok(None);
    };
    let event_count = u64::try_from(event_count).map_err(|_| {
        StorageError::MigrationInvariant("projection event_count is negative".to_owned())
    })?;
    Ok(Some(JournalProjection {
        project_id: project_id.to_owned(),
        event_count,
        last_sequence: Some(last_sequence),
        last_event_id: Some(last_event_id),
        rolling_digest,
    }))
}
