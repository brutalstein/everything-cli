//! Deterministic hierarchical repository capsules derived from RI2.
//!
//! Capsules are navigation/localization projections over the existing index. They
//! are not a second index and never replace exact source for an edit decision.

use std::{collections::BTreeSet, path::Path};

use rusqlite::{OptionalExtension, params};

use super::{CapabilityTier, FreshnessState, RepositoryIndex, stable_id};
use crate::RepoError;

const CAPSULE_PRODUCER_VERSION: &str = "ri2-capsule-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryCapsuleKind {
    Repository,
    Package,
    Directory,
    File,
    Symbol,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryCapsuleScope {
    Repository,
    Package(String),
    Directory(String),
    File(String),
    Symbol(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepositoryCapsuleLimits {
    pub max_symbols: usize,
    pub max_dependencies: usize,
    pub max_dependents: usize,
    pub max_tests: usize,
    pub max_build_targets: usize,
    pub max_source_anchors: usize,
    pub max_source_hashes: usize,
}

impl Default for RepositoryCapsuleLimits {
    fn default() -> Self {
        Self {
            max_symbols: 24,
            max_dependencies: 24,
            max_dependents: 24,
            max_tests: 16,
            max_build_targets: 16,
            max_source_anchors: 24,
            max_source_hashes: 32,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapsuleSymbol {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapsuleRelation {
    pub kind: String,
    pub identity: String,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapsuleSourceAnchor {
    pub path: String,
    pub line: Option<u32>,
    pub content_sha256: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryCapsule {
    pub capsule_id: String,
    pub snapshot_id: String,
    pub kind: RepositoryCapsuleKind,
    pub canonical_identity: String,
    pub primary_role: String,
    pub key_symbols: Vec<CapsuleSymbol>,
    pub dependencies: Vec<CapsuleRelation>,
    pub dependents: Vec<CapsuleRelation>,
    pub tests: Vec<String>,
    pub build_targets: Vec<String>,
    pub source_anchors: Vec<CapsuleSourceAnchor>,
    pub source_hashes: Vec<String>,
    pub capability_tier: CapabilityTier,
    pub freshness: FreshnessState,
    pub producer_version: String,
}

impl RepositoryIndex {
    pub fn repository_capsule(
        &self,
        snapshot_id: &str,
        scope: &RepositoryCapsuleScope,
        limits: RepositoryCapsuleLimits,
    ) -> Result<RepositoryCapsule, RepoError> {
        validate_limits(limits)?;
        self.ensure_snapshot(snapshot_id)?;
        let resolved = self.resolve_capsule_scope(snapshot_id, scope)?;
        let key_symbols = self.capsule_symbols(snapshot_id, &resolved, limits.max_symbols)?;
        let dependencies =
            self.capsule_graph_relations(snapshot_id, &resolved, true, limits.max_dependencies)?;
        let dependents =
            self.capsule_graph_relations(snapshot_id, &resolved, false, limits.max_dependents)?;
        let tests = self.capsule_tests(snapshot_id, &resolved, limits.max_tests)?;
        let build_targets =
            self.capsule_build_targets(snapshot_id, &resolved, limits.max_build_targets)?;
        let source_anchors =
            self.capsule_source_anchors(snapshot_id, &resolved, limits.max_source_anchors)?;
        let source_hashes =
            self.capsule_source_hashes(snapshot_id, &resolved, limits.max_source_hashes)?;
        let capability_tier = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(capability_tier),0) FROM ri2_view_state WHERE snapshot_id=?",
                [snapshot_id],
                |row| row.get::<_, i64>(0),
            )
            .map(CapabilityTier::from_i64)??;
        let current: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM current_snapshots WHERE snapshot_id=?)",
            [snapshot_id],
            |row| row.get(0),
        )?;
        let freshness = if current {
            FreshnessState::Current
        } else {
            FreshnessState::Stale
        };
        let kind_name = capsule_kind_name(resolved.kind);
        let capsule_id = stable_id("capsule", &[kind_name, &resolved.canonical_identity]);

        Ok(RepositoryCapsule {
            capsule_id,
            snapshot_id: snapshot_id.to_owned(),
            kind: resolved.kind,
            canonical_identity: resolved.canonical_identity.clone(),
            primary_role: deterministic_primary_role(&resolved),
            key_symbols,
            dependencies,
            dependents,
            tests,
            build_targets,
            source_anchors,
            source_hashes,
            capability_tier,
            freshness,
            producer_version: CAPSULE_PRODUCER_VERSION.to_owned(),
        })
    }

    fn resolve_capsule_scope(
        &self,
        snapshot_id: &str,
        scope: &RepositoryCapsuleScope,
    ) -> Result<ResolvedScope, RepoError> {
        match scope {
            RepositoryCapsuleScope::Repository => Ok(ResolvedScope {
                kind: RepositoryCapsuleKind::Repository,
                canonical_identity: "repository".to_owned(),
                path_prefix: None,
                exact_path: None,
                symbol_id: None,
                package_id: None,
            }),
            RepositoryCapsuleScope::Package(package_id) => {
                let manifest_path: String = self
                    .connection
                    .query_row(
                        "SELECT manifest_path FROM ri2_build_packages WHERE snapshot_id=? AND package_id=?",
                        params![snapshot_id, package_id],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| RepoError::Integrity(format!("unknown RI2 package: {package_id}")))?;
                let prefix = Path::new(&manifest_path)
                    .parent()
                    .and_then(Path::to_str)
                    .unwrap_or("")
                    .replace('\\', "/");
                Ok(ResolvedScope {
                    kind: RepositoryCapsuleKind::Package,
                    canonical_identity: package_id.clone(),
                    path_prefix: (!prefix.is_empty()).then_some(prefix),
                    exact_path: None,
                    symbol_id: None,
                    package_id: Some(package_id.clone()),
                })
            }
            RepositoryCapsuleScope::Directory(path) => {
                let normalized = normalized_scope_path(path)?;
                Ok(ResolvedScope {
                    kind: RepositoryCapsuleKind::Directory,
                    canonical_identity: normalized.clone(),
                    path_prefix: Some(normalized),
                    exact_path: None,
                    symbol_id: None,
                    package_id: None,
                })
            }
            RepositoryCapsuleScope::File(path) => {
                let normalized = normalized_scope_path(path)?;
                let exists: bool = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM snapshot_files WHERE snapshot_id=? AND path=?)",
                    params![snapshot_id, normalized],
                    |row| row.get(0),
                )?;
                if !exists {
                    return Err(RepoError::Integrity(format!(
                        "unknown indexed file: {normalized}"
                    )));
                }
                Ok(ResolvedScope {
                    kind: RepositoryCapsuleKind::File,
                    canonical_identity: normalized.clone(),
                    path_prefix: None,
                    exact_path: Some(normalized),
                    symbol_id: None,
                    package_id: None,
                })
            }
            RepositoryCapsuleScope::Symbol(symbol_id) => {
                let row: Option<(String, String)> = self.connection.query_row(
                    "SELECT path,label FROM ri2_graph_nodes WHERE snapshot_id=? AND node_id=? AND node_kind='symbol'",
                    params![snapshot_id, symbol_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                ).optional()?;
                let (path, label) = row.ok_or_else(|| {
                    RepoError::Integrity(format!("unknown RI2 symbol: {symbol_id}"))
                })?;
                Ok(ResolvedScope {
                    kind: RepositoryCapsuleKind::Symbol,
                    canonical_identity: format!("{path}::{label}"),
                    path_prefix: None,
                    exact_path: Some(path),
                    symbol_id: Some(symbol_id.clone()),
                    package_id: None,
                })
            }
        }
    }

    fn capsule_symbols(
        &self,
        snapshot_id: &str,
        scope: &ResolvedScope,
        limit: usize,
    ) -> Result<Vec<CapsuleSymbol>, RepoError> {
        let mut statement = self.connection.prepare(
            "SELECT s.name,s.kind,f.path,s.start_line,s.end_line,s.signature
             FROM snapshot_files f
             JOIN content_symbols s ON s.content_sha256=f.content_sha256 AND s.parser_key=f.parser_key
             WHERE f.snapshot_id=? ORDER BY f.path,s.start_line,s.name",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok(CapsuleSymbol {
                name: row.get(0)?,
                kind: row.get(1)?,
                path: row.get(2)?,
                start_line: row.get(3)?,
                end_line: row.get(4)?,
                signature: row.get(5)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            let symbol = row?;
            if scope.matches_path(&symbol.path) {
                result.push(symbol);
                if result.len() >= limit {
                    break;
                }
            }
        }
        Ok(result)
    }

    fn capsule_graph_relations(
        &self,
        snapshot_id: &str,
        scope: &ResolvedScope,
        outgoing: bool,
        limit: usize,
    ) -> Result<Vec<CapsuleRelation>, RepoError> {
        let (scope_alias, peer_alias, source_column, target_column) = if outgoing {
            ("s", "t", "source_node_id", "target_node_id")
        } else {
            ("t", "s", "target_node_id", "source_node_id")
        };
        let sql = format!(
            "SELECT e.edge_kind,p.node_id,p.label,p.path,{scope_alias}.path
             FROM ri2_graph_edges e
             JOIN ri2_graph_nodes {scope_alias} ON {scope_alias}.snapshot_id=e.snapshot_id AND {scope_alias}.node_id=e.{source_column}
             JOIN ri2_graph_nodes {peer_alias} p ON p.snapshot_id=e.snapshot_id AND p.node_id=e.{target_column}
             WHERE e.snapshot_id=? ORDER BY e.edge_kind,p.label,p.node_id"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        for row in rows {
            let (kind, peer_id, peer_label, peer_path, scope_path) = row?;
            if !scope.matches_optional_path(scope_path.as_deref()) {
                continue;
            }
            let identity = if peer_label.trim().is_empty() {
                peer_id
            } else {
                peer_label
            };
            if seen.insert((kind.clone(), identity.clone(), peer_path.clone())) {
                result.push(CapsuleRelation {
                    kind,
                    identity,
                    path: peer_path,
                });
                if result.len() >= limit {
                    break;
                }
            }
        }
        Ok(result)
    }

    fn capsule_tests(
        &self,
        snapshot_id: &str,
        scope: &ResolvedScope,
        limit: usize,
    ) -> Result<Vec<String>, RepoError> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT test_path,target_path FROM test_links WHERE snapshot_id=? ORDER BY test_path,target_path",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut tests = Vec::new();
        for row in rows {
            let (test_path, target_path) = row?;
            if scope.matches_path(&target_path) || scope.matches_path(&test_path) {
                tests.push(test_path);
                tests.sort();
                tests.dedup();
                if tests.len() >= limit {
                    break;
                }
            }
        }
        Ok(tests)
    }

    fn capsule_build_targets(
        &self,
        snapshot_id: &str,
        scope: &ResolvedScope,
        limit: usize,
    ) -> Result<Vec<String>, RepoError> {
        let mut statement = self.connection.prepare(
            "SELECT target_id,package_id,name,kind,source_path FROM ri2_build_targets WHERE snapshot_id=? ORDER BY package_id,name,target_id",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (target_id, package_id, name, kind, source_path) = row?;
            let matches = scope.package_id.as_deref() == Some(package_id.as_str())
                || source_path
                    .as_deref()
                    .is_some_and(|path| scope.matches_path(path))
                || scope.kind == RepositoryCapsuleKind::Repository;
            if matches {
                result.push(format!("{kind}:{name}:{target_id}"));
                if result.len() >= limit {
                    break;
                }
            }
        }
        Ok(result)
    }

    fn capsule_source_anchors(
        &self,
        snapshot_id: &str,
        scope: &ResolvedScope,
        limit: usize,
    ) -> Result<Vec<CapsuleSourceAnchor>, RepoError> {
        let mut statement = self.connection.prepare(
            "SELECT path,source_line,content_sha256,node_id FROM ri2_graph_nodes WHERE snapshot_id=? ORDER BY path,source_line,node_id",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<u32>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            let (path, line, hash, node_id) = row?;
            let Some(path) = path else { continue };
            if !scope.matches_path(&path)
                || scope
                    .symbol_id
                    .as_deref()
                    .is_some_and(|symbol_id| symbol_id != node_id)
            {
                continue;
            }
            result.push(CapsuleSourceAnchor {
                path,
                line,
                content_sha256: hash,
            });
            if result.len() >= limit {
                break;
            }
        }
        Ok(result)
    }

    fn capsule_source_hashes(
        &self,
        snapshot_id: &str,
        scope: &ResolvedScope,
        limit: usize,
    ) -> Result<Vec<String>, RepoError> {
        let mut statement = self.connection.prepare(
            "SELECT path,content_sha256 FROM snapshot_files WHERE snapshot_id=? AND content_sha256 IS NOT NULL ORDER BY path",
        )?;
        let rows = statement.query_map([snapshot_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut hashes = Vec::new();
        for row in rows {
            let (path, hash) = row?;
            if scope.matches_path(&path) {
                hashes.push(hash);
                hashes.sort();
                hashes.dedup();
                if hashes.len() >= limit {
                    break;
                }
            }
        }
        Ok(hashes)
    }
}

#[derive(Clone, Debug)]
struct ResolvedScope {
    kind: RepositoryCapsuleKind,
    canonical_identity: String,
    path_prefix: Option<String>,
    exact_path: Option<String>,
    symbol_id: Option<String>,
    package_id: Option<String>,
}

impl ResolvedScope {
    fn matches_path(&self, path: &str) -> bool {
        if self.kind == RepositoryCapsuleKind::Repository {
            return true;
        }
        if let Some(exact) = &self.exact_path {
            return exact == path;
        }
        self.path_prefix.as_deref().is_some_and(|prefix| {
            path == prefix
                || path
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    }

    fn matches_optional_path(&self, path: Option<&str>) -> bool {
        self.kind == RepositoryCapsuleKind::Repository
            || path.is_some_and(|path| self.matches_path(path))
    }
}

fn normalized_scope_path(path: &str) -> Result<String, RepoError> {
    if path.trim().is_empty() || path.contains('\\') || path.contains('\0') {
        return Err(RepoError::InvalidRelativePath(path.to_owned()));
    }
    let path_value = Path::new(path);
    if path_value.is_absolute()
        || path_value
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(RepoError::InvalidRelativePath(path.to_owned()));
    }
    Ok(path.trim_end_matches('/').to_owned())
}

fn deterministic_primary_role(scope: &ResolvedScope) -> String {
    match scope.kind {
        RepositoryCapsuleKind::Repository => "repository navigation root".to_owned(),
        RepositoryCapsuleKind::Package => "build/package boundary".to_owned(),
        RepositoryCapsuleKind::Directory => "directory/module navigation boundary".to_owned(),
        RepositoryCapsuleKind::File => "indexed source file".to_owned(),
        RepositoryCapsuleKind::Symbol => "exact indexed symbol location".to_owned(),
    }
}

fn capsule_kind_name(kind: RepositoryCapsuleKind) -> &'static str {
    match kind {
        RepositoryCapsuleKind::Repository => "repository",
        RepositoryCapsuleKind::Package => "package",
        RepositoryCapsuleKind::Directory => "directory",
        RepositoryCapsuleKind::File => "file",
        RepositoryCapsuleKind::Symbol => "symbol",
    }
}

fn validate_limits(limits: RepositoryCapsuleLimits) -> Result<(), RepoError> {
    if limits.max_symbols == 0
        || limits.max_dependencies == 0
        || limits.max_dependents == 0
        || limits.max_tests == 0
        || limits.max_build_targets == 0
        || limits.max_source_anchors == 0
        || limits.max_source_hashes == 0
    {
        return Err(RepoError::InvalidPolicy);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(prefix: &str) -> ResolvedScope {
        ResolvedScope {
            kind: RepositoryCapsuleKind::Directory,
            canonical_identity: prefix.to_owned(),
            path_prefix: Some(prefix.to_owned()),
            exact_path: None,
            symbol_id: None,
            package_id: None,
        }
    }

    #[test]
    fn directory_scope_is_boundary_aware_not_string_prefix_based() {
        let capsule = scope("crates/aer-core");
        assert!(capsule.matches_path("crates/aer-core/src/lib.rs"));
        assert!(!capsule.matches_path("crates/aer-core-old/src/lib.rs"));
    }

    #[test]
    fn capsule_identity_does_not_include_snapshot_identity() {
        let left = stable_id("capsule", &["file", "src/lib.rs"]);
        let right = stable_id("capsule", &["file", "src/lib.rs"]);
        assert_eq!(left, right);
    }
}
