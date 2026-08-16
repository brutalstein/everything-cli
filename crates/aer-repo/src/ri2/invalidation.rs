use std::collections::BTreeSet;

use crate::{RepoError, RepositoryIndex, validate_relative};

use super::{GraphDirection, RepositoryChangeSet, TraversalBudget, file_node_id};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidationFrontier {
    pub changes: RepositoryChangeSet,
    pub invalidated_entity_ids: Vec<String>,
    pub truncated: bool,
}

/// Returns the stable RI2 entity identifier for a repository-relative file path.
///
/// The identifier is deliberately snapshot-independent; freshness and truth are carried by the
/// snapshot-bound graph/evidence records that reference it.
pub fn repository_file_entity_id(path: &str) -> Result<String, RepoError> {
    validate_relative(path)?;
    Ok(file_node_id(path))
}

impl RepositoryIndex {
    /// Computes a bounded dependency-aware invalidation frontier from the previous snapshot graph.
    /// Directly changed/deleted files are always included. Graph expansion is conservative and
    /// explicitly reports truncation rather than pretending an unbounded impact set was computed.
    pub fn invalidation_frontier(
        &self,
        from_snapshot: &str,
        to_snapshot: &str,
        budget: TraversalBudget,
    ) -> Result<InvalidationFrontier, RepoError> {
        let budget = budget.validate()?;
        let changes = self.diff_snapshots(from_snapshot, to_snapshot)?;
        let mut entities = changes
            .invalidated_entity_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut truncated = entities.len() > budget.max_nodes;

        for root in &changes.invalidated_entity_ids {
            if entities.len() >= budget.max_nodes {
                truncated = true;
                break;
            }
            let remaining_nodes = budget.max_nodes.saturating_sub(entities.len()).max(1);
            let traversal = self.graph_traverse(
                from_snapshot,
                std::slice::from_ref(root),
                GraphDirection::Both,
                TraversalBudget {
                    max_depth: budget.max_depth,
                    max_nodes: remaining_nodes,
                    max_edges: budget.max_edges,
                },
            )?;
            truncated |= traversal.truncated;
            for node in traversal.nodes {
                if entities.len() >= budget.max_nodes {
                    truncated = true;
                    break;
                }
                entities.insert(node.node_id);
            }
        }

        let mut invalidated_entity_ids = entities.into_iter().collect::<Vec<_>>();
        if invalidated_entity_ids.len() > budget.max_nodes {
            invalidated_entity_ids.truncate(budget.max_nodes);
            truncated = true;
        }
        Ok(InvalidationFrontier {
            changes,
            invalidated_entity_ids,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_file_entity_rejects_workspace_escape() {
        assert!(repository_file_entity_id("src/auth.rs").is_ok());
        assert!(repository_file_entity_id("../outside.rs").is_err());
    }
}
