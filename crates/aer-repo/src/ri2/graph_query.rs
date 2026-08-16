use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::{RepoError, RepositoryIndex, SearchQuery};

use super::{
    CapabilityTier, EdgeEvidence, EvidenceClass, FreshnessState, GraphDirection, GraphEdge,
    GraphEdgeKind, GraphNode, GraphNodeKind, GraphQueryResult, LanguageCapabilityReport,
    ProjectDependency, RepositoryChangeSet, Ri2RetrievalHit, SymbolContinuity, TraversalBudget,
    ViewState, file_node_id,
};
use super::model::BuildPackage;

impl RepositoryIndex {
    pub fn ri2_view_states(&self, snapshot_id: &str) -> Result<Vec<ViewState>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut statement = self.connection.prepare(
            "SELECT view_name,producer_id,producer_version,freshness,capability_tier FROM ri2_view_state WHERE snapshot_id=? ORDER BY view_name",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok((
                row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                row.get::<_, String>(3)?, row.get::<_, i64>(4)?,
            ))
        })?;
        let mut output = Vec::new();
        for row in rows {
            let (view_name, producer_id, producer_version, freshness, tier) = row?;
            output.push(ViewState {
                view_name,
                indexed_snapshot: snapshot_id.to_owned(),
                producer_id,
                producer_version,
                freshness: FreshnessState::parse(&freshness)?,
                capability_tier: CapabilityTier::from_i64(tier)?,
            });
        }
        Ok(output)
    }

    pub fn language_capability_report(&self, snapshot_id: &str) -> Result<LanguageCapabilityReport, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let text_files = count(&self.connection, "SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='text'", snapshot_id)?;
        let tier1 = count(&self.connection, "SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='text' AND parser_key LIKE 'tree-sitter-%'", snapshot_id)?;
        let tier2 = count(&self.connection, "SELECT COUNT(DISTINCT source_path) FROM ri2_build_targets WHERE snapshot_id=? AND source_path IS NOT NULL", snapshot_id)?;
        let tier3 = count(&self.connection, "SELECT COUNT(DISTINCT source_path) FROM ri2_graph_edges WHERE snapshot_id=? AND evidence_class='semantic_resolved' AND source_path IS NOT NULL", snapshot_id)?;
        let tier4 = count(&self.connection, "SELECT COUNT(DISTINCT path) FROM runtime_links WHERE snapshot_id=?", snapshot_id)?;
        let fallback = count(&self.connection, "SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='text' AND language='other'", snapshot_id)?;
        Ok(LanguageCapabilityReport {
            registry_version: crate::language::LANGUAGE_REGISTRY_VERSION.to_owned(),
            text_files,
            tier0_files: text_files,
            tier1_files: tier1,
            tier2_files: tier2,
            tier3_files: tier3,
            tier4_files: tier4,
            fallback_files: fallback,
            ambiguous_files: 0,
        })
    }

    pub fn build_packages(&self, snapshot_id: &str) -> Result<Vec<BuildPackage>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut statement = self.connection.prepare(
            "SELECT package_id,manager,name,version,manifest_path,workspace_member FROM ri2_build_packages WHERE snapshot_id=? ORDER BY name,package_id",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok(BuildPackage {
                package_id: row.get(0)?, manager: row.get(1)?, name: row.get(2)?, version: row.get(3)?,
                manifest_path: row.get(4)?, workspace_member: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RepoError::from)
    }

    pub fn project_dependencies(&self, snapshot_id: &str) -> Result<Vec<ProjectDependency>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut statement = self.connection.prepare(
            "SELECT source_package_id,target_name,target_package_id,dependency_kind,manifest_path FROM ri2_project_dependencies WHERE snapshot_id=? ORDER BY source_package_id,target_name,dependency_kind",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok(ProjectDependency {
                source_package_id: row.get(0)?, target_name: row.get(1)?, target_package_id: row.get(2)?,
                dependency_kind: row.get(3)?, manifest_path: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RepoError::from)
    }

    pub fn graph_traverse(
        &self,
        snapshot_id: &str,
        roots: &[String],
        direction: GraphDirection,
        budget: TraversalBudget,
    ) -> Result<GraphQueryResult, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let budget = budget.validate()?;
        for root in roots {
            require_graph_node(&self.connection, snapshot_id, root)?;
        }
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::new();
        for root in roots {
            seen.insert(root.clone());
            queue.push_back((root.clone(), 0_u16));
        }
        let mut edges = Vec::new();
        let mut truncated = false;
        while let Some((node, depth)) = queue.pop_front() {
            if depth >= budget.max_depth {
                continue;
            }
            for edge in adjacent_edges(&self.connection, snapshot_id, &node, direction)? {
                if edges.len() >= budget.max_edges {
                    truncated = true;
                    break;
                }
                let neighbor = if edge.source_node_id == node {
                    edge.target_node_id.clone()
                } else {
                    edge.source_node_id.clone()
                };
                edges.push(edge);
                if seen.insert(neighbor.clone()) {
                    if seen.len() > budget.max_nodes {
                        seen.remove(&neighbor);
                        truncated = true;
                        break;
                    }
                    queue.push_back((neighbor, depth.saturating_add(1)));
                }
            }
            if truncated {
                break;
            }
        }
        let mut nodes = Vec::new();
        for node_id in &seen {
            if let Some(node) = graph_node(&self.connection, snapshot_id, node_id)? {
                nodes.push(node);
            }
        }
        nodes.sort_by(|left, right| left.node_id.cmp(&right.node_id));
        edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
        edges.dedup_by(|left, right| left.edge_id == right.edge_id);
        Ok(GraphQueryResult {
            snapshot_id: snapshot_id.to_owned(),
            root_node_ids: roots.to_vec(),
            nodes,
            edges,
            truncated,
        })
    }

    pub fn backlinks(&self, snapshot_id: &str, node_id: &str, budget: TraversalBudget) -> Result<GraphQueryResult, RepoError> {
        self.graph_traverse(snapshot_id, &[node_id.to_owned()], GraphDirection::Incoming, budget)
    }

    pub fn hybrid_retrieve(
        &self,
        snapshot_id: &str,
        query: &SearchQuery,
        expansion_budget: TraversalBudget,
    ) -> Result<Vec<Ri2RetrievalHit>, RepoError> {
        let lexical = self.search(snapshot_id, query)?;
        let freshness = self.ri2_view_states(snapshot_id)?.into_iter()
            .find(|view| view.view_name == "graph")
            .map_or(FreshnessState::Unavailable, |view| view.freshness);
        let mut hits: BTreeMap<String, Ri2RetrievalHit> = BTreeMap::new();
        for hit in lexical.hits {
            hits.insert(hit.path.clone(), Ri2RetrievalHit {
                path: hit.path.clone(),
                why_relevant: vec!["lexical_or_symbol_match".to_owned()],
                capability_tier: file_capability_tier(&self.connection, snapshot_id, &hit.path)?,
                provenance: vec![EvidenceClass::Extracted],
                freshness,
                confidence_milli: 1000,
            });
            let graph = self.graph_traverse(
                snapshot_id,
                &[file_node_id(&hit.path)],
                GraphDirection::Both,
                TraversalBudget { max_depth: expansion_budget.max_depth.min(1), ..expansion_budget },
            )?;
            for node in graph.nodes {
                let Some(path) = node.path else { continue; };
                if path == hit.path { continue; }
                let entry = hits.entry(path.clone()).or_insert(Ri2RetrievalHit {
                    path: path.clone(),
                    why_relevant: Vec::new(),
                    capability_tier: file_capability_tier(&self.connection, snapshot_id, &path)?,
                    provenance: Vec::new(),
                    freshness,
                    confidence_milli: 700,
                });
                entry.why_relevant.push(format!("bounded_graph_neighbor:{}", hit.path));
            }
            for edge in graph.edges {
                if let Some(path) = edge.evidence.source_path.as_deref()
                    && let Some(entry) = hits.get_mut(path)
                    && !entry.provenance.contains(&edge.evidence.evidence_class)
                {
                    entry.provenance.push(edge.evidence.evidence_class);
                }
            }
        }
        let mut output = hits.into_values().collect::<Vec<_>>();
        output.sort_by(|left, right| right.confidence_milli.cmp(&left.confidence_milli).then_with(|| left.path.cmp(&right.path)));
        output.truncate(query.limit.max(1));
        Ok(output)
    }

    pub fn diff_snapshots(&self, from_snapshot: &str, to_snapshot: &str) -> Result<RepositoryChangeSet, RepoError> {
        self.ensure_snapshot(from_snapshot)?;
        self.ensure_snapshot(to_snapshot)?;
        let from = snapshot_file_hashes(&self.connection, from_snapshot)?;
        let to = snapshot_file_hashes(&self.connection, to_snapshot)?;
        let mut added = Vec::new();
        let mut changed = Vec::new();
        let mut deleted = Vec::new();
        let mut invalidated = Vec::new();
        for (path, hash) in &from {
            match to.get(path) {
                None => { deleted.push(path.clone()); invalidated.push(file_node_id(path)); }
                Some(next) if next != hash => { changed.push(path.clone()); invalidated.push(file_node_id(path)); }
                _ => {}
            }
        }
        for path in to.keys() {
            if !from.contains_key(path) { added.push(path.clone()); }
        }
        Ok(RepositoryChangeSet {
            from_snapshot: from_snapshot.to_owned(), to_snapshot: to_snapshot.to_owned(),
            added_paths: added, changed_paths: changed, deleted_paths: deleted,
            invalidated_entity_ids: invalidated,
        })
    }

    pub fn symbol_continuity(&self, repo_id: &str, from_snapshot: &str, to_snapshot: &str) -> Result<Vec<SymbolContinuity>, RepoError> {
        let mut statement = self.connection.prepare(
            "SELECT logical_symbol_id,from_symbol_id,to_symbol_id,evidence_class,confidence_milli FROM ri2_symbol_continuity WHERE repo_id=? AND from_snapshot=? AND to_snapshot=? ORDER BY logical_symbol_id,from_symbol_id,to_symbol_id",
        )?;
        let rows = statement.query_map(params![repo_id, from_snapshot, to_snapshot], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, u16>(4)?))
        })?;
        let mut output = Vec::new();
        for row in rows {
            let (logical, from_symbol, to_symbol, evidence, confidence) = row?;
            output.push(SymbolContinuity {
                logical_symbol_id: logical,
                from_snapshot: from_snapshot.to_owned(), from_symbol_id: from_symbol,
                to_snapshot: to_snapshot.to_owned(), to_symbol_id: to_symbol,
                evidence_class: EvidenceClass::parse(&evidence)?, confidence_milli: confidence,
            });
        }
        Ok(output)
    }
}

pub(crate) fn load_edge(
    tx: &Transaction<'_>, snapshot_id: &str, source: &str, target: &str,
    kind: GraphEdgeKind, evidence: EvidenceClass, producer_id: &str,
) -> Result<GraphEdge, RepoError> {
    let mut statement = tx.prepare(
        "SELECT edge_id,source_node_id,target_node_id,edge_kind,evidence_class,confidence_milli,producer_id,producer_version,source_path,source_line,environment_fingerprint,valid_from_snapshot,valid_until_snapshot FROM ri2_graph_edges WHERE snapshot_id=? AND source_node_id=? AND target_node_id=? AND edge_kind=? AND evidence_class=? AND producer_id=? ORDER BY edge_id DESC LIMIT 1",
    )?;
    let mut rows = statement.query(params![snapshot_id, source, target, kind.as_str(), evidence.as_str(), producer_id])?;
    let row = rows.next()?.ok_or_else(|| RepoError::Integrity("inserted RI2 edge could not be reloaded".to_owned()))?;
    graph_edge_from_row(snapshot_id, row)
}

fn count(connection: &Connection, sql: &str, snapshot_id: &str) -> Result<usize, RepoError> {
    let value: i64 = connection.query_row(sql, [snapshot_id], |row| row.get(0))?;
    usize::try_from(value).map_err(|_| RepoError::Integrity(format!("negative RI2 count: {value}")))
}

fn require_graph_node(connection: &Connection, snapshot_id: &str, node_id: &str) -> Result<(), RepoError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM ri2_graph_nodes WHERE snapshot_id=? AND node_id=?)",
        params![snapshot_id, node_id], |row| row.get(0),
    )?;
    if exists { Ok(()) } else { Err(RepoError::Integrity(format!("RI2 graph node does not exist in snapshot: {node_id}"))) }
}

fn graph_node(connection: &Connection, snapshot_id: &str, node_id: &str) -> Result<Option<GraphNode>, RepoError> {
    let row = connection.query_row(
        "SELECT node_kind,label,path,source_line,content_sha256 FROM ri2_graph_nodes WHERE snapshot_id=? AND node_id=?",
        params![snapshot_id, node_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, Option<u32>>(3)?, row.get::<_, Option<String>>(4)?)),
    ).optional()?;
    row.map(|(kind, label, path, line, hash)| Ok(GraphNode {
        node_id: node_id.to_owned(), kind: GraphNodeKind::parse(&kind)?, label, path, source_line: line, content_sha256: hash,
    })).transpose()
}

fn adjacent_edges(connection: &Connection, snapshot_id: &str, node_id: &str, direction: GraphDirection) -> Result<Vec<GraphEdge>, RepoError> {
    let (sql, both) = match direction {
        GraphDirection::Outgoing => ("SELECT edge_id,source_node_id,target_node_id,edge_kind,evidence_class,confidence_milli,producer_id,producer_version,source_path,source_line,environment_fingerprint,valid_from_snapshot,valid_until_snapshot FROM ri2_graph_edges WHERE snapshot_id=? AND source_node_id=? ORDER BY edge_id", false),
        GraphDirection::Incoming => ("SELECT edge_id,source_node_id,target_node_id,edge_kind,evidence_class,confidence_milli,producer_id,producer_version,source_path,source_line,environment_fingerprint,valid_from_snapshot,valid_until_snapshot FROM ri2_graph_edges WHERE snapshot_id=? AND target_node_id=? ORDER BY edge_id", false),
        GraphDirection::Both => ("SELECT edge_id,source_node_id,target_node_id,edge_kind,evidence_class,confidence_milli,producer_id,producer_version,source_path,source_line,environment_fingerprint,valid_from_snapshot,valid_until_snapshot FROM ri2_graph_edges WHERE snapshot_id=? AND (source_node_id=? OR target_node_id=?) ORDER BY edge_id", true),
    };
    let mut statement = connection.prepare(sql)?;
    let mut rows = if both { statement.query(params![snapshot_id, node_id, node_id])? } else { statement.query(params![snapshot_id, node_id])? };
    let mut output = Vec::new();
    while let Some(row) = rows.next()? { output.push(graph_edge_from_row(snapshot_id, row)?); }
    Ok(output)
}

fn graph_edge_from_row(snapshot_id: &str, row: &rusqlite::Row<'_>) -> Result<GraphEdge, RepoError> {
    Ok(GraphEdge {
        edge_id: row.get(0)?, source_node_id: row.get(1)?, target_node_id: row.get(2)?,
        kind: GraphEdgeKind::parse(&row.get::<_, String>(3)?)?,
        evidence: EdgeEvidence {
            evidence_class: EvidenceClass::parse(&row.get::<_, String>(4)?)?, confidence_milli: row.get(5)?,
            producer_id: row.get(6)?, producer_version: row.get(7)?, repo_snapshot: snapshot_id.to_owned(),
            source_path: row.get(8)?, source_line: row.get(9)?, environment_fingerprint: row.get(10)?,
            valid_from_snapshot: row.get(11)?, valid_until_snapshot: row.get(12)?,
        },
    })
}

fn file_capability_tier(connection: &Connection, snapshot_id: &str, path: &str) -> Result<CapabilityTier, RepoError> {
    let parser: Option<String> = connection.query_row(
        "SELECT parser_key FROM snapshot_files WHERE snapshot_id=? AND path=?", params![snapshot_id, path], |row| row.get(0),
    )?;
    let precise: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM ri2_graph_edges WHERE snapshot_id=? AND source_path=? AND evidence_class='semantic_resolved')",
        params![snapshot_id, path], |row| row.get(0),
    )?;
    if precise { return Ok(CapabilityTier::Tier3PreciseSemantic); }
    let project: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM ri2_build_targets WHERE snapshot_id=? AND source_path=?)",
        params![snapshot_id, path], |row| row.get(0),
    )?;
    if project { return Ok(CapabilityTier::Tier2Project); }
    if parser.is_some_and(|key| key.starts_with("tree-sitter-")) { Ok(CapabilityTier::Tier1Syntax) } else { Ok(CapabilityTier::Tier0Text) }
}

fn snapshot_file_hashes(connection: &Connection, snapshot_id: &str) -> Result<BTreeMap<String, Option<String>>, RepoError> {
    let mut statement = connection.prepare("SELECT path,content_sha256 FROM snapshot_files WHERE snapshot_id=? ORDER BY path")?;
    let rows = statement.query_map([snapshot_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))?;
    rows.collect::<Result<BTreeMap<_, _>, _>>().map_err(RepoError::from)
}
