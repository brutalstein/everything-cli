use aer_environment::EnvironmentFingerprint;
use rusqlite::OptionalExtension;

use crate::{RepoError, RepositoryIndex};

use super::build::{CARGO_PRODUCER, CARGO_PRODUCER_VERSION};
use super::graph_write::{GRAPH_PRODUCER, GRAPH_PRODUCER_VERSION};

impl RepositoryIndex {
    pub(crate) fn ri2_snapshot_requires_rebuild(
        &self,
        snapshot_id: &str,
    ) -> Result<bool, RepoError> {
        self.ensure_snapshot(snapshot_id)?;

        for view_name in ["syntax", "graph"] {
            let producer = self
                .connection
                .query_row(
                    "SELECT producer_id,producer_version FROM ri2_view_state WHERE snapshot_id=? AND view_name=?",
                    rusqlite::params![snapshot_id, view_name],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if producer.as_ref().is_none_or(|(id, version)| {
                id != GRAPH_PRODUCER || version != GRAPH_PRODUCER_VERSION
            }) {
                return Ok(true);
            }
        }

        let project_producer = self
    .connection
    .query_row(
        "SELECT producer_id,producer_version,environment_fingerprint FROM ri2_view_state WHERE snapshot_id=? AND view_name='project'",
        [snapshot_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )
    .optional()?;
        if project_producer.as_ref().is_none_or(|(id, version, _)| {
            id != CARGO_PRODUCER || version != CARGO_PRODUCER_VERSION
        }) {
            return Ok(true);
        }
        if let Some((_, _, Some(stored_environment))) = project_producer {
            let repo_root: String = self.connection.query_row(
                "SELECT repo_root FROM snapshots WHERE snapshot_id=?",
                [snapshot_id],
                |row| row.get(0),
            )?;
            let current_environment = match EnvironmentFingerprint::discover(repo_root) {
                Ok(fingerprint) => fingerprint.digest,
                Err(_) => return Ok(true),
            };
            if current_environment != stored_environment {
                return Ok(true);
            }
        }

        let mut statement = self.connection.prepare(
            "SELECT path,parser_key FROM snapshot_files WHERE snapshot_id=? AND file_kind='text' ORDER BY path",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (path, parser_key) = row?;
            let language = crate::syntax::detect_language(&path);
            let expected = crate::syntax::parser_key(language);
            if parser_key.as_deref() != Some(expected.as_str()) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
