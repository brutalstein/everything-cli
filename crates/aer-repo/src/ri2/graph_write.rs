use rusqlite::{Connection, Transaction, params};

use crate::{PreparedFile, RepoError, RepoSnapshotIdentity, symbol_id};

use super::build::{CARGO_PRODUCER, CARGO_PRODUCER_VERSION};
use super::{
    BuildTopology, CapabilityTier, EvidenceClass, FreshnessState, GraphEdgeKind, GraphNodeKind,
    PreciseRelation, PreciseSemanticBatch, RepositoryIndex, file_node_id, package_node_id,
    stable_id,
};

pub(crate) const GRAPH_PRODUCER: &str = "aer-ri2-graph";
pub(crate) const GRAPH_PRODUCER_VERSION: &str = "1";

pub(crate) fn rebuild_snapshot_views(
    tx: &Transaction<'_>,
    snapshot: &RepoSnapshotIdentity,
    previous_snapshot: Option<&str>,
    prepared: &[PreparedFile],
    topology: &BuildTopology,
) -> Result<(), RepoError> {
    let snapshot_id = snapshot.snapshot_id.as_str();
    for table in [
        "ri2_graph_edges",
        "ri2_graph_nodes",
        "ri2_build_targets",
        "ri2_project_dependencies",
        "ri2_build_packages",
        "ri2_view_state",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE snapshot_id=?"),
            [snapshot_id],
        )?;
    }

    insert_view_state(
        tx,
        snapshot_id,
        "lexical",
        "aer-lexical",
        "2",
        FreshnessState::Current,
        CapabilityTier::Tier0Text,
    )?;
    insert_view_state(
        tx,
        snapshot_id,
        "syntax",
        GRAPH_PRODUCER,
        GRAPH_PRODUCER_VERSION,
        FreshnessState::Current,
        CapabilityTier::Tier1Syntax,
    )?;
    insert_project_view_state(
        tx,
        snapshot_id,
        CARGO_PRODUCER,
        CARGO_PRODUCER_VERSION,
        topology.environment_fingerprint.as_deref(),
        topology.state,
    )?;
    insert_view_state(
        tx,
        snapshot_id,
        "precise_semantic",
        "external-semantic-adapter",
        "1",
        FreshnessState::Unavailable,
        CapabilityTier::Tier3PreciseSemantic,
    )?;
    let runtime_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM runtime_links WHERE snapshot_id=?",
        [snapshot_id],
        |row| row.get(0),
    )?;
    insert_view_state(
        tx,
        snapshot_id,
        "runtime",
        "runtime-observation",
        "1",
        if runtime_count > 0 {
            FreshnessState::Current
        } else {
            FreshnessState::Unavailable
        },
        CapabilityTier::Tier4DynamicEvidence,
    )?;
    insert_view_state(
        tx,
        snapshot_id,
        "graph",
        GRAPH_PRODUCER,
        GRAPH_PRODUCER_VERSION,
        FreshnessState::Current,
        CapabilityTier::Tier1Syntax,
    )?;

    for file in prepared {
        GraphNodeWriter::new(tx, snapshot_id).insert(
            &file_node_id(&file.path),
            if file.is_test {
                GraphNodeKind::Test
            } else {
                GraphNodeKind::File
            },
            &file.path,
            Some(&file.path),
            None,
            file.content_sha256.as_deref(),
        )?;
    }
    rebuild_symbol_graph(tx, snapshot_id)?;
    rebuild_test_graph(tx, snapshot_id)?;
    rebuild_semantic_anchor_graph(tx, snapshot_id)?;
    rebuild_runtime_graph(tx, snapshot_id)?;
    insert_build_topology(tx, snapshot_id, topology)?;
    if let Some(previous_snapshot) = previous_snapshot {
        rebuild_exact_continuity(tx, snapshot, previous_snapshot)?;
    }
    Ok(())
}

pub(crate) fn insert_view_state(
    tx: &Transaction<'_>,
    snapshot_id: &str,
    view_name: &str,
    producer_id: &str,
    producer_version: &str,
    freshness: FreshnessState,
    tier: CapabilityTier,
) -> Result<(), RepoError> {
    tx.execute(
        "INSERT INTO ri2_view_state(snapshot_id,view_name,producer_id,producer_version,freshness,capability_tier) VALUES(?,?,?,?,?,?)",
        params![snapshot_id, view_name, producer_id, producer_version, freshness.as_str(), i64::from(tier.as_u8())],
    )?;
    Ok(())
}

fn insert_project_view_state(
    tx: &Transaction<'_>,
    snapshot_id: &str,
    producer_id: &str,
    producer_version: &str,
    environment_fingerprint: Option<&str>,
    freshness: FreshnessState,
) -> Result<(), RepoError> {
    tx.execute(
        "INSERT INTO ri2_view_state(snapshot_id,view_name,producer_id,producer_version,environment_fingerprint,freshness,capability_tier) VALUES(?,?,?,?,?,?,?)",
        params![
            snapshot_id,
            "project",
            producer_id,
            producer_version,
            environment_fingerprint,
            freshness.as_str(),
            i64::from(CapabilityTier::Tier2Project.as_u8())
        ],
    )?;
    Ok(())
}

struct GraphNodeWriter<'tx, 'conn> {
    tx: &'tx Transaction<'conn>,
    snapshot_id: &'tx str,
}

impl<'tx, 'conn> GraphNodeWriter<'tx, 'conn> {
    fn new(tx: &'tx Transaction<'conn>, snapshot_id: &'tx str) -> Self {
        Self { tx, snapshot_id }
    }

    fn insert(
        &self,
        node_id: &str,
        kind: GraphNodeKind,
        label: &str,
        path: Option<&str>,
        source_line: Option<u32>,
        content_sha256: Option<&str>,
    ) -> Result<(), RepoError> {
        self.tx.execute(
            "INSERT OR IGNORE INTO ri2_graph_nodes(snapshot_id,node_id,node_kind,label,path,source_line,content_sha256) VALUES(?,?,?,?,?,?,?)",
            params![self.snapshot_id, node_id, kind.as_str(), label, path, source_line, content_sha256],
        )?;
        Ok(())
    }
}

pub(crate) struct NewGraphEdge<'a> {
    pub source: &'a str,
    pub target: &'a str,
    pub kind: GraphEdgeKind,
    pub evidence_class: EvidenceClass,
    pub confidence_milli: u16,
    pub producer_id: &'a str,
    pub producer_version: &'a str,
    pub source_path: Option<&'a str>,
    pub source_line: Option<u32>,
    pub environment_fingerprint: Option<&'a str>,
}

pub(crate) fn insert_edge(
    tx: &Transaction<'_>,
    snapshot_id: &str,
    edge: NewGraphEdge<'_>,
) -> Result<(), RepoError> {
    let line = edge
        .source_line
        .map_or_else(String::new, |value| value.to_string());
    let edge_id = stable_id(
        "graph-edge",
        &[
            snapshot_id,
            edge.source,
            edge.target,
            edge.kind.as_str(),
            edge.evidence_class.as_str(),
            edge.producer_id,
            &line,
        ],
    );
    tx.execute(
        "INSERT OR IGNORE INTO ri2_graph_edges(snapshot_id,edge_id,source_node_id,target_node_id,edge_kind,evidence_class,confidence_milli,producer_id,producer_version,source_path,source_line,environment_fingerprint,valid_from_snapshot,valid_until_snapshot) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,NULL)",
        params![
            snapshot_id,
            edge_id,
            edge.source,
            edge.target,
            edge.kind.as_str(),
            edge.evidence_class.as_str(),
            i64::from(edge.confidence_milli),
            edge.producer_id,
            edge.producer_version,
            edge.source_path,
            edge.source_line,
            edge.environment_fingerprint,
            snapshot_id
        ],
    )?;
    Ok(())
}

fn rebuild_symbol_graph(tx: &Transaction<'_>, snapshot_id: &str) -> Result<(), RepoError> {
    let mut symbols = tx.prepare(
        "SELECT f.path,f.content_sha256,f.parser_key,s.local_id,s.name,s.start_line FROM snapshot_files f JOIN content_symbols s ON s.content_sha256=f.content_sha256 AND s.parser_key=f.parser_key WHERE f.snapshot_id=? ORDER BY f.path,s.start_byte",
    )?;
    for row in symbols.query_map([snapshot_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, u32>(5)?,
        ))
    })? {
        let (path, hash, parser, local, name, line) = row?;
        let id = symbol_id(&path, &hash, &local);
        GraphNodeWriter::new(tx, snapshot_id).insert(
            &id,
            GraphNodeKind::Symbol,
            &name,
            Some(&path),
            Some(line),
            Some(&hash),
        )?;
        insert_edge(
            tx,
            snapshot_id,
            NewGraphEdge {
                source: &file_node_id(&path),
                target: &id,
                kind: GraphEdgeKind::Defines,
                evidence_class: EvidenceClass::Extracted,
                confidence_milli: 1000,
                producer_id: &parser,
                producer_version: "content-artifact",
                source_path: Some(&path),
                source_line: Some(line),
                environment_fingerprint: None,
            },
        )?;
    }

    let mut links = tx.prepare(
        "SELECT f.path,f.content_sha256,f.parser_key,l.source_local_id,l.kind,l.target_name,l.line FROM snapshot_files f JOIN content_links l ON l.content_sha256=f.content_sha256 AND l.parser_key=f.parser_key WHERE f.snapshot_id=? ORDER BY f.path,l.line,l.kind,l.target_name",
    )?;
    for row in links.query_map([snapshot_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, u32>(6)?,
        ))
    })? {
        let (path, hash, parser, source_local, kind, target_name, line) = row?;
        let source = source_local
            .as_deref()
            .map(|local| symbol_id(&path, &hash, local))
            .unwrap_or_else(|| file_node_id(&path));
        let target = stable_id("symbol-candidate", &[&target_name]);
        GraphNodeWriter::new(tx, snapshot_id).insert(
            &target,
            GraphNodeKind::SymbolCandidate,
            &target_name,
            None,
            None,
            None,
        )?;
        let edge_kind = match kind.as_str() {
            "imports" => GraphEdgeKind::Imports,
            "calls" => GraphEdgeKind::Calls,
            _ => GraphEdgeKind::References,
        };
        insert_edge(
            tx,
            snapshot_id,
            NewGraphEdge {
                source: &source,
                target: &target,
                kind: edge_kind,
                evidence_class: EvidenceClass::Extracted,
                confidence_milli: 1000,
                producer_id: &parser,
                producer_version: "content-artifact",
                source_path: Some(&path),
                source_line: Some(line),
                environment_fingerprint: None,
            },
        )?;
    }
    Ok(())
}

fn rebuild_test_graph(tx: &Transaction<'_>, snapshot_id: &str) -> Result<(), RepoError> {
    let mut links = tx.prepare(
        "SELECT test_path,target_path,target_symbol_id,confidence_milli FROM test_links WHERE snapshot_id=? ORDER BY test_path,target_path",
    )?;
    for row in links.query_map([snapshot_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, u16>(3)?,
        ))
    })? {
        let (test_path, target_path, target_symbol, confidence) = row?;
        insert_edge(
            tx,
            snapshot_id,
            NewGraphEdge {
                source: &file_node_id(&test_path),
                target: &target_symbol.unwrap_or_else(|| file_node_id(&target_path)),
                kind: GraphEdgeKind::Tests,
                evidence_class: EvidenceClass::Inferred,
                confidence_milli: confidence,
                producer_id: "aer-test-association",
                producer_version: "1",
                source_path: Some(&test_path),
                source_line: None,
                environment_fingerprint: None,
            },
        )?;
    }
    Ok(())
}

fn rebuild_semantic_anchor_graph(tx: &Transaction<'_>, snapshot_id: &str) -> Result<(), RepoError> {
    let mut links = tx.prepare(
        "SELECT semantic_kind,semantic_id,target_path,score_micros FROM semantic_links WHERE snapshot_id=? ORDER BY semantic_id,target_path",
    )?;
    for row in links.query_map([snapshot_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })? {
        let (kind, id, path, score) = row?;
        let semantic_node = stable_id("semantic", &[&kind, &id]);
        GraphNodeWriter::new(tx, snapshot_id).insert(
            &semantic_node,
            GraphNodeKind::SemanticAnchor,
            &id,
            None,
            None,
            None,
        )?;
        let confidence = u16::try_from((score / 1_000_000).clamp(0, 1000)).unwrap_or(0);
        insert_edge(
            tx,
            snapshot_id,
            NewGraphEdge {
                source: &semantic_node,
                target: &file_node_id(&path),
                kind: GraphEdgeKind::Supports,
                evidence_class: EvidenceClass::Inferred,
                confidence_milli: confidence,
                producer_id: "aer-semantic-link",
                producer_version: "1",
                source_path: Some(&path),
                source_line: None,
                environment_fingerprint: None,
            },
        )?;
    }
    Ok(())
}

fn rebuild_runtime_graph(tx: &Transaction<'_>, snapshot_id: &str) -> Result<(), RepoError> {
    let mut links = tx.prepare(
        "SELECT observation_id,path,line,summary FROM runtime_links WHERE snapshot_id=? ORDER BY observation_id",
    )?;
    for row in links.query_map([snapshot_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<u32>>(2)?,
            row.get::<_, String>(3)?,
        ))
    })? {
        let (observation, path, line, summary) = row?;
        let observation_node = stable_id("runtime", &[&observation]);
        GraphNodeWriter::new(tx, snapshot_id).insert(
            &observation_node,
            GraphNodeKind::RuntimeObservation,
            &summary,
            Some(&path),
            line,
            None,
        )?;
        insert_edge(
            tx,
            snapshot_id,
            NewGraphEdge {
                source: &observation_node,
                target: &file_node_id(&path),
                kind: GraphEdgeKind::ObservedIn,
                evidence_class: EvidenceClass::Observed,
                confidence_milli: 1000,
                producer_id: "runtime-observation",
                producer_version: "1",
                source_path: Some(&path),
                source_line: line,
                environment_fingerprint: None,
            },
        )?;
    }
    Ok(())
}

fn insert_build_topology(
    tx: &Transaction<'_>,
    snapshot_id: &str,
    topology: &BuildTopology,
) -> Result<(), RepoError> {
    for package in &topology.packages {
        tx.execute(
            "INSERT INTO ri2_build_packages(snapshot_id,package_id,manager,name,version,manifest_path,workspace_member) VALUES(?,?,?,?,?,?,?)",
            params![snapshot_id, package.package_id, package.manager, package.name, package.version, package.manifest_path, if package.workspace_member { 1 } else { 0 }],
        )?;
        let package_node = package_node_id(&package.package_id);
        GraphNodeWriter::new(tx, snapshot_id).insert(
            &package_node,
            GraphNodeKind::Package,
            &package.name,
            Some(&package.manifest_path),
            None,
            None,
        )?;
        insert_edge(
            tx,
            snapshot_id,
            NewGraphEdge {
                source: &file_node_id(&package.manifest_path),
                target: &package_node,
                kind: GraphEdgeKind::Defines,
                evidence_class: EvidenceClass::Extracted,
                confidence_milli: 1000,
                producer_id: CARGO_PRODUCER,
                producer_version: CARGO_PRODUCER_VERSION,
                source_path: Some(&package.manifest_path),
                source_line: None,
                environment_fingerprint: None,
            },
        )?;
    }
    for target in &topology.targets {
        tx.execute(
            "INSERT INTO ri2_build_targets(snapshot_id,target_id,package_id,name,kind,source_path) VALUES(?,?,?,?,?,?)",
            params![snapshot_id, target.target_id, target.package_id, target.name, target.kind, target.source_path],
        )?;
        let node = stable_id("build-target-node", &[&target.target_id]);
        GraphNodeWriter::new(tx, snapshot_id).insert(
            &node,
            GraphNodeKind::BuildTarget,
            &target.name,
            target.source_path.as_deref(),
            None,
            None,
        )?;
        insert_edge(
            tx,
            snapshot_id,
            NewGraphEdge {
                source: &package_node_id(&target.package_id),
                target: &node,
                kind: GraphEdgeKind::Builds,
                evidence_class: EvidenceClass::Extracted,
                confidence_milli: 1000,
                producer_id: CARGO_PRODUCER,
                producer_version: CARGO_PRODUCER_VERSION,
                source_path: target.source_path.as_deref(),
                source_line: None,
                environment_fingerprint: None,
            },
        )?;
        if let Some(path) = target.source_path.as_deref() {
            insert_edge(
                tx,
                snapshot_id,
                NewGraphEdge {
                    source: &node,
                    target: &file_node_id(path),
                    kind: GraphEdgeKind::Builds,
                    evidence_class: EvidenceClass::Extracted,
                    confidence_milli: 1000,
                    producer_id: CARGO_PRODUCER,
                    producer_version: CARGO_PRODUCER_VERSION,
                    source_path: Some(path),
                    source_line: None,
                    environment_fingerprint: None,
                },
            )?;
        }
    }
    for dependency in &topology.dependencies {
        tx.execute(
            "INSERT INTO ri2_project_dependencies(snapshot_id,source_package_id,target_name,target_package_id,dependency_kind,manifest_path) VALUES(?,?,?,?,?,?)",
            params![snapshot_id, dependency.source_package_id, dependency.target_name, dependency.target_package_id, dependency.dependency_kind, dependency.manifest_path],
        )?;
        let target_node = dependency.target_package_id.as_deref().map_or_else(
            || stable_id("external-package", &[&dependency.target_name]),
            package_node_id,
        );
        if dependency.target_package_id.is_none() {
            GraphNodeWriter::new(tx, snapshot_id).insert(
                &target_node,
                GraphNodeKind::ExternalPackage,
                &dependency.target_name,
                None,
                None,
                None,
            )?;
        }
        insert_edge(
            tx,
            snapshot_id,
            NewGraphEdge {
                source: &package_node_id(&dependency.source_package_id),
                target: &target_node,
                kind: GraphEdgeKind::DependsOn,
                evidence_class: EvidenceClass::Extracted,
                confidence_milli: 1000,
                producer_id: CARGO_PRODUCER,
                producer_version: CARGO_PRODUCER_VERSION,
                source_path: Some(&dependency.manifest_path),
                source_line: None,
                environment_fingerprint: None,
            },
        )?;
    }
    Ok(())
}

fn rebuild_exact_continuity(
    tx: &Transaction<'_>,
    snapshot: &RepoSnapshotIdentity,
    previous_snapshot: &str,
) -> Result<(), RepoError> {
    let mut rows = tx.prepare(
        "SELECT old.path,old.content_sha256,new.path,new.content_sha256,os.local_id,ns.local_id
         FROM snapshot_files old
         JOIN snapshot_files new ON new.snapshot_id=? AND new.content_sha256=old.content_sha256 AND new.parser_key=old.parser_key
         JOIN content_symbols os ON os.content_sha256=old.content_sha256 AND os.parser_key=old.parser_key
         JOIN content_symbols ns ON ns.content_sha256=new.content_sha256 AND ns.parser_key=new.parser_key AND ns.local_id=os.local_id
         WHERE old.snapshot_id=? AND old.content_sha256 IS NOT NULL
         ORDER BY old.path,new.path,os.local_id",
    )?;
    for row in rows.query_map(params![snapshot.snapshot_id, previous_snapshot], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })? {
        let (old_path, old_hash, new_path, new_hash, old_local, new_local) = row?;
        let old_symbol = symbol_id(&old_path, &old_hash, &old_local);
        let new_symbol = symbol_id(&new_path, &new_hash, &new_local);
        let logical = stable_id("logical-symbol", &[&old_hash, &old_local]);
        tx.execute(
            "INSERT OR IGNORE INTO ri2_symbol_continuity(repo_id,logical_symbol_id,from_snapshot,from_symbol_id,to_snapshot,to_symbol_id,evidence_class,confidence_milli,producer_id) VALUES(?,?,?,?,?,?,?,?,?)",
            params![snapshot.repo_id, logical, previous_snapshot, old_symbol, snapshot.snapshot_id, new_symbol, EvidenceClass::Extracted.as_str(), 1000_i64, "content-identity"],
        )?;
        if old_path != new_path {
            let historical = stable_id("historical-file", &[previous_snapshot, &old_path]);
            GraphNodeWriter::new(tx, &snapshot.snapshot_id).insert(
                &historical,
                GraphNodeKind::File,
                &old_path,
                None,
                None,
                Some(&old_hash),
            )?;
            insert_edge(
                tx,
                &snapshot.snapshot_id,
                NewGraphEdge {
                    source: &file_node_id(&new_path),
                    target: &historical,
                    kind: GraphEdgeKind::RenamedFrom,
                    evidence_class: EvidenceClass::Extracted,
                    confidence_milli: 1000,
                    producer_id: "content-identity",
                    producer_version: "1",
                    source_path: Some(&new_path),
                    source_line: None,
                    environment_fingerprint: None,
                },
            )?;
        }
    }
    Ok(())
}

impl RepositoryIndex {
    pub fn ingest_precise_semantics(
        &mut self,
        batch: &PreciseSemanticBatch,
    ) -> Result<Vec<super::GraphEdge>, RepoError> {
        self.ensure_snapshot(&batch.snapshot_id)?;
        if batch.producer_id.trim().is_empty()
            || batch.producer_version.trim().is_empty()
            || batch.environment_fingerprint.trim().is_empty()
        {
            return Err(RepoError::Integrity(
                "precise semantic ingestion requires producer and environment identity".to_owned(),
            ));
        }
        let tx = self.connection.transaction()?;
        let mut output = Vec::new();
        for record in &batch.edges {
            crate::validate_relative(&record.source_path)?;
            require_snapshot_path(&tx, &batch.snapshot_id, &record.source_path)?;
            if let Some(target_path) = record.target_path.as_deref() {
                crate::validate_relative(target_path)?;
                require_snapshot_path(&tx, &batch.snapshot_id, target_path)?;
            }
            let source = record
                .source_symbol_id
                .clone()
                .unwrap_or_else(|| file_node_id(&record.source_path));
            if record.source_symbol_id.is_some() {
                require_graph_node(&tx, &batch.snapshot_id, &source)?;
            }
            let target = stable_id("precise-symbol", &[&record.target_symbol]);
            GraphNodeWriter::new(&tx, &batch.snapshot_id).insert(
                &target,
                GraphNodeKind::Symbol,
                &record.target_symbol,
                record.target_path.as_deref(),
                None,
                None,
            )?;
            let kind = match record.relation {
                PreciseRelation::Definition => GraphEdgeKind::ResolvesTo,
                PreciseRelation::Reference => GraphEdgeKind::References,
                PreciseRelation::Call => GraphEdgeKind::Calls,
                PreciseRelation::Implementation => GraphEdgeKind::Implements,
                PreciseRelation::Inheritance => GraphEdgeKind::Inherits,
            };
            insert_edge(
                &tx,
                &batch.snapshot_id,
                NewGraphEdge {
                    source: &source,
                    target: &target,
                    kind,
                    evidence_class: EvidenceClass::SemanticResolved,
                    confidence_milli: 1000,
                    producer_id: &batch.producer_id,
                    producer_version: &batch.producer_version,
                    source_path: Some(&record.source_path),
                    source_line: record.source_line,
                    environment_fingerprint: Some(&batch.environment_fingerprint),
                },
            )?;
            output.push(super::graph_query::load_edge(
                &tx,
                &batch.snapshot_id,
                &source,
                &target,
                kind,
                EvidenceClass::SemanticResolved,
                &batch.producer_id,
            )?);
        }
        tx.execute(
            "UPDATE ri2_view_state SET producer_id=?,producer_version=?,freshness='current' WHERE snapshot_id=? AND view_name='precise_semantic'",
            params![batch.producer_id, batch.producer_version, batch.snapshot_id],
        )?;
        tx.commit()?;
        Ok(output)
    }
}

fn require_snapshot_path(
    connection: &Connection,
    snapshot_id: &str,
    path: &str,
) -> Result<(), RepoError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM snapshot_files WHERE snapshot_id=? AND path=?)",
        params![snapshot_id, path],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(RepoError::Integrity(format!(
            "semantic adapter referenced path outside exact snapshot: {path}"
        )))
    }
}

fn require_graph_node(
    connection: &Connection,
    snapshot_id: &str,
    node_id: &str,
) -> Result<(), RepoError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM ri2_graph_nodes WHERE snapshot_id=? AND node_id=?)",
        params![snapshot_id, node_id],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(RepoError::Integrity(format!(
            "RI2 graph node does not exist in snapshot: {node_id}"
        )))
    }
}
