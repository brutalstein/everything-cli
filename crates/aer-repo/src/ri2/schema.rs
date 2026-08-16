use rusqlite::Connection;

use crate::RepoError;

pub(crate) const RI2_SCHEMA_VERSION: i64 = 2;

pub(crate) fn migrate_v1_to_v2(connection: &Connection) -> Result<(), RepoError> {
    debug_assert_eq!(RI2_SCHEMA_VERSION, 2);
    connection.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE IF NOT EXISTS ri2_view_state(
           snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,
           view_name TEXT NOT NULL,
           producer_id TEXT NOT NULL,
           producer_version TEXT NOT NULL,
           freshness TEXT NOT NULL,
           capability_tier INTEGER NOT NULL,
           PRIMARY KEY(snapshot_id,view_name)
         );
         CREATE TABLE IF NOT EXISTS ri2_graph_nodes(
           snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,
           node_id TEXT NOT NULL,
           node_kind TEXT NOT NULL,
           label TEXT NOT NULL,
           path TEXT,
           source_line INTEGER,
           content_sha256 TEXT,
           PRIMARY KEY(snapshot_id,node_id)
         );
         CREATE INDEX IF NOT EXISTS ri2_graph_nodes_path ON ri2_graph_nodes(snapshot_id,path);
         CREATE TABLE IF NOT EXISTS ri2_graph_edges(
           snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,
           edge_id TEXT NOT NULL,
           source_node_id TEXT NOT NULL,
           target_node_id TEXT NOT NULL,
           edge_kind TEXT NOT NULL,
           evidence_class TEXT NOT NULL,
           confidence_milli INTEGER NOT NULL,
           producer_id TEXT NOT NULL,
           producer_version TEXT NOT NULL,
           source_path TEXT,
           source_line INTEGER,
           environment_fingerprint TEXT,
           valid_from_snapshot TEXT NOT NULL,
           valid_until_snapshot TEXT,
           PRIMARY KEY(snapshot_id,edge_id)
         );
         CREATE INDEX IF NOT EXISTS ri2_graph_edges_source ON ri2_graph_edges(snapshot_id,source_node_id);
         CREATE INDEX IF NOT EXISTS ri2_graph_edges_target ON ri2_graph_edges(snapshot_id,target_node_id);
         CREATE TABLE IF NOT EXISTS ri2_build_packages(
           snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,
           package_id TEXT NOT NULL,
           manager TEXT NOT NULL,
           name TEXT NOT NULL,
           version TEXT NOT NULL,
           manifest_path TEXT NOT NULL,
           workspace_member INTEGER NOT NULL,
           PRIMARY KEY(snapshot_id,package_id)
         );
         CREATE TABLE IF NOT EXISTS ri2_build_targets(
           snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,
           target_id TEXT NOT NULL,
           package_id TEXT NOT NULL,
           name TEXT NOT NULL,
           kind TEXT NOT NULL,
           source_path TEXT,
           PRIMARY KEY(snapshot_id,target_id)
         );
         CREATE TABLE IF NOT EXISTS ri2_project_dependencies(
           snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,
           source_package_id TEXT NOT NULL,
           target_name TEXT NOT NULL,
           target_package_id TEXT,
           dependency_kind TEXT NOT NULL,
           manifest_path TEXT NOT NULL,
           PRIMARY KEY(snapshot_id,source_package_id,target_name,dependency_kind)
         );
         CREATE TABLE IF NOT EXISTS ri2_symbol_continuity(
           repo_id TEXT NOT NULL,
           logical_symbol_id TEXT NOT NULL,
           from_snapshot TEXT NOT NULL,
           from_symbol_id TEXT NOT NULL,
           to_snapshot TEXT NOT NULL,
           to_symbol_id TEXT NOT NULL,
           evidence_class TEXT NOT NULL,
           confidence_milli INTEGER NOT NULL,
           producer_id TEXT NOT NULL,
           PRIMARY KEY(repo_id,from_snapshot,from_symbol_id,to_snapshot,to_symbol_id)
         );
         PRAGMA user_version=2;
         COMMIT;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ri2_schema_version_is_v2() {
        assert_eq!(RI2_SCHEMA_VERSION, 2);
    }
}
