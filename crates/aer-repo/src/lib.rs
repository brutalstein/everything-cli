//! Commit-aware, bounded, derived repository intelligence.
//!
//! This crate never owns project authority. It indexes an exact workspace snapshot and refuses
//! current-workspace retrieval when that snapshot no longer matches.

mod language;
mod model;
mod ri2;
mod syntax;

pub use model::*;
pub use ri2::*;
pub use syntax::{SourceMeasurement, SourceUnit, measure_source};

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};

use aer_exec::{CommandSpec, ExecutionPolicy, LocalProcessExecutor, SideEffectClass};
use aer_workspace::{SnapshotPolicy, WorkspaceSnapshot};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};

use crate::syntax::{detect_language, is_test_path, parse_text, parser_key, tokenize};

const INDEX_SCHEMA_VERSION: i64 = 3;

pub struct RepositoryIndex {
    connection: Connection,
    policy: IndexPolicy,
}

impl RepositoryIndex {
    pub fn open(path: impl AsRef<Path>, policy: IndexPolicy) -> Result<Self, RepoError> {
        policy.validate()?;
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "trusted_schema", "OFF")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        initialize_schema(&connection)?;
        Ok(Self { connection, policy })
    }

    pub fn refresh(
        &mut self,
        workspace_root: impl AsRef<Path>,
    ) -> Result<IndexBuildReport, RepoError> {
        let before =
            WorkspaceSnapshot::capture(workspace_root.as_ref(), &SnapshotPolicy::default())?;
        let snapshot = snapshot_identity(&before)?;
        let previous_snapshot = self.current_snapshot_id(&snapshot.repo_id)?;
        let snapshot_unchanged = previous_snapshot.as_deref() == Some(&snapshot.snapshot_id);
        if snapshot_unchanged && !self.ri2_snapshot_requires_rebuild(&snapshot.snapshot_id)? {
            return self.report_existing(snapshot, true);
        }
        let continuity_snapshot = if snapshot_unchanged {
            None
        } else {
            previous_snapshot.as_deref()
        };

        let paths = list_repository_files(&before.identity.repo_root, &self.policy)?;
        if paths.len() > self.policy.max_files {
            return Err(RepoError::FileLimitExceeded(paths.len()));
        }
        let untracked: BTreeMap<String, Vec<u8>> = before
            .untracked_files
            .iter()
            .map(|file| path_string(&file.relative_path).map(|path| (path, file.bytes.clone())))
            .collect::<Result<_, _>>()?;

        let mut prepared = Vec::with_capacity(paths.len());
        let mut missing_paths = BTreeSet::new();
        let mut total_text = 0_u64;
        for relative in paths {
            let full = before.identity.repo_root.join(&relative);
            let metadata = match fs::symlink_metadata(&full) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing_paths.insert(relative);
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let language = detect_language(&relative);
            let test = is_test_path(&relative);
            if metadata.file_type().is_symlink() {
                prepared.push(PreparedFile::unindexed(
                    relative,
                    metadata.len(),
                    language,
                    FileKind::Symlink,
                    test,
                ));
                continue;
            }
            if !metadata.file_type().is_file() {
                continue;
            }
            if metadata.len() > self.policy.max_text_file_bytes {
                prepared.push(PreparedFile::unindexed(
                    relative,
                    metadata.len(),
                    language,
                    FileKind::Oversized,
                    test,
                ));
                continue;
            }
            let bytes = match untracked.get(&relative) {
                Some(bytes) => bytes.clone(),
                None => match fs::read(&full) {
                    Ok(bytes) => bytes,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        missing_paths.insert(relative);
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                },
            };
            total_text = total_text
                .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                .ok_or(RepoError::TextBudgetExceeded(u64::MAX))?;
            if total_text > self.policy.max_total_text_bytes {
                return Err(RepoError::TextBudgetExceeded(total_text));
            }
            let content_sha256 = sha256(&bytes);
            let Ok(text) = std::str::from_utf8(&bytes) else {
                prepared.push(PreparedFile {
                    path: relative,
                    byte_len: metadata.len(),
                    language,
                    kind: FileKind::Binary,
                    is_test: test,
                    content_sha256: Some(content_sha256),
                    parser_key: None,
                    text: None,
                });
                continue;
            };
            if bytes.contains(&0) {
                prepared.push(PreparedFile {
                    path: relative,
                    byte_len: metadata.len(),
                    language,
                    kind: FileKind::Binary,
                    is_test: test,
                    content_sha256: Some(content_sha256),
                    parser_key: None,
                    text: None,
                });
                continue;
            }
            prepared.push(PreparedFile {
                path: relative,
                byte_len: metadata.len(),
                language,
                kind: FileKind::Text,
                is_test: test,
                content_sha256: Some(content_sha256),
                parser_key: Some(parser_key(language)),
                text: Some(text.to_owned()),
            });
        }

        let build_topology =
            ri2::collect_project_topology(&before.identity.repo_root, &self.policy);
        let after =
            WorkspaceSnapshot::capture(&before.identity.repo_root, &SnapshotPolicy::default())?;
        if snapshot_identity(&after)?.snapshot_id != snapshot.snapshot_id {
            return Err(RepoError::WorkspaceChangedDuringIndex);
        }
        for relative in &missing_paths {
            match fs::symlink_metadata(before.identity.repo_root.join(relative)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => return Err(RepoError::WorkspaceChangedDuringIndex),
                Err(error) => return Err(error.into()),
            }
        }

        let git_view = collect_git_view(&before.identity.repo_root, &self.policy)?;
        let transaction = self.connection.transaction()?;
        clear_snapshot_materialized_views(&transaction, &snapshot.snapshot_id, snapshot_unchanged)?;
        insert_snapshot(&transaction, &snapshot)?;
        let mut parsed_artifacts = 0_usize;
        let mut reused_artifacts = 0_usize;
        let mut symbols = 0_usize;
        let mut edges = 0_usize;

        for file in &prepared {
            let mut line_count = 0_u32;
            if let (Some(hash), Some(key), Some(text)) = (
                file.content_sha256.as_deref(),
                file.parser_key.as_deref(),
                file.text.as_deref(),
            ) {
                if artifact_exists(&transaction, hash, key)? {
                    reused_artifacts += 1;
                } else {
                    let artifact = parse_text(&file.path, text, file.language, &self.policy)?;
                    line_count = artifact.line_count;
                    insert_artifact(&transaction, hash, key, &artifact)?;
                    parsed_artifacts += 1;
                }
                if line_count == 0 {
                    line_count = artifact_line_count(&transaction, hash, key)?;
                }
                symbols += artifact_symbol_count(&transaction, hash, key)?;
                edges += artifact_link_count(&transaction, hash, key)?;
            }
            transaction.execute(
                "INSERT INTO snapshot_files(snapshot_id,path,content_sha256,parser_key,byte_len,line_count,language,file_kind,is_test) VALUES(?,?,?,?,?,?,?,?,?)",
                params![
                    snapshot.snapshot_id,
                    file.path,
                    file.content_sha256,
                    file.parser_key,
                    i64::try_from(file.byte_len).unwrap_or(i64::MAX),
                    i64::from(line_count),
                    file.language.as_str(),
                    file.kind.as_str(),
                    if file.is_test { 1 } else { 0 },
                ],
            )?;
        }
        insert_git_view(&transaction, &snapshot.snapshot_id, &git_view)?;
        let test_associations = rebuild_test_links(&transaction, &snapshot.snapshot_id, 100)?;
        ri2::rebuild_snapshot_views(
            &transaction,
            &snapshot,
            continuity_snapshot,
            &prepared,
            &build_topology,
        )?;
        transaction.execute(
            "INSERT INTO current_snapshots(repo_id,snapshot_id) VALUES(?,?) ON CONFLICT(repo_id) DO UPDATE SET snapshot_id=excluded.snapshot_id",
            params![snapshot.repo_id, snapshot.snapshot_id],
        )?;
        prune_snapshots(
            &transaction,
            &snapshot.repo_id,
            self.policy.retained_snapshots,
        )?;
        garbage_collect_artifacts(&transaction)?;
        transaction.commit()?;

        Ok(IndexBuildReport {
            snapshot,
            already_current: false,
            files_seen: prepared.len(),
            text_files: prepared
                .iter()
                .filter(|file| file.kind == FileKind::Text)
                .count(),
            binary_files: prepared
                .iter()
                .filter(|file| file.kind == FileKind::Binary)
                .count(),
            oversized_files: prepared
                .iter()
                .filter(|file| file.kind == FileKind::Oversized)
                .count(),
            symlinks: prepared
                .iter()
                .filter(|file| file.kind == FileKind::Symlink)
                .count(),
            parsed_artifacts,
            reused_artifacts,
            symbols,
            dependency_edges: edges,
            test_associations,
            git_commits: git_view.commits.len(),
            cochange_pairs: git_view.cochanges.len(),
        })
    }

    pub fn search(
        &self,
        snapshot_id: &str,
        query: &SearchQuery,
    ) -> Result<SearchResult, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        validate_query(query, &self.policy)?;
        let terms = tokenize(&query.text, 256)?;
        if terms.is_empty() {
            return Ok(SearchResult {
                snapshot_id: snapshot_id.to_owned(),
                hits: Vec::new(),
                abstained: true,
                abstention_reason: Some(AbstentionReason::EmptyQuery),
            });
        }

        let document_count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='text'",
            [snapshot_id],
            |row| row.get(0),
        )?;
        if document_count == 0 {
            return Ok(SearchResult {
                snapshot_id: snapshot_id.to_owned(),
                hits: Vec::new(),
                abstained: true,
                abstention_reason: Some(AbstentionReason::NoIndexedTerms),
            });
        }
        let total_tokens: i64 = self.connection.query_row(
            "SELECT COALESCE(SUM(a.token_count),0) FROM snapshot_files f JOIN content_artifacts a ON a.content_sha256=f.content_sha256 AND a.parser_key=f.parser_key WHERE f.snapshot_id=?",
            [snapshot_id], |row| row.get(0)
        )?;
        let avg_len = (total_tokens.max(1) as f64) / (document_count as f64);
        let mut scored: BTreeMap<String, HitAccumulator> = BTreeMap::new();
        let mut matched_any = false;

        for term in &terms {
            let df: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM snapshot_files f JOIN content_terms t ON t.content_sha256=f.content_sha256 AND t.parser_key=f.parser_key WHERE f.snapshot_id=? AND t.term=?",
                params![snapshot_id, term], |row| row.get(0)
            )?;
            if df == 0 {
                continue;
            }
            matched_any = true;
            let idf = (1.0 + ((document_count as f64 - df as f64 + 0.5) / (df as f64 + 0.5))).ln();
            let mut statement = self.connection.prepare(
                "SELECT f.path,f.content_sha256,f.language,t.tf,t.first_line,a.token_count FROM snapshot_files f JOIN content_terms t ON t.content_sha256=f.content_sha256 AND t.parser_key=f.parser_key JOIN content_artifacts a ON a.content_sha256=f.content_sha256 AND a.parser_key=f.parser_key WHERE f.snapshot_id=? AND t.term=?"
            )?;
            let rows = statement.query_map(params![snapshot_id, term], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?;
            for row in rows {
                let (path, hash, language, tf, first_line, doc_len) = row?;
                let tf = tf as f64;
                let length_norm = (doc_len as f64) / avg_len;
                let score = idf * ((tf * 2.2) / (tf + 1.2 * (0.25 + 0.75 * length_norm)));
                let hit = scored.entry(path.clone()).or_insert_with(|| {
                    HitAccumulator::new(hash, parse_language(&language), first_line)
                });
                hit.score += score;
                hit.anchor_line = Some(
                    hit.anchor_line
                        .map_or(first_line, |line| line.min(first_line)),
                );
                hit.terms.insert(term.clone());
                if path.to_ascii_lowercase().contains(term) {
                    hit.score += 0.4;
                }
            }
        }

        for term in &terms {
            let mut statement = self.connection.prepare(
                "SELECT f.path,f.content_sha256,f.language,s.name,s.start_line FROM snapshot_files f JOIN content_symbols s ON s.content_sha256=f.content_sha256 AND s.parser_key=f.parser_key WHERE f.snapshot_id=? AND lower(s.name)=?"
            )?;
            let rows = statement.query_map(params![snapshot_id, term], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                ))
            })?;
            for row in rows {
                let (path, hash, language, symbol, line) = row?;
                matched_any = true;
                let hit = scored
                    .entry(path)
                    .or_insert_with(|| HitAccumulator::new(hash, parse_language(&language), line));
                hit.score += 5.0;
                hit.symbols.insert(symbol);
            }
        }

        if !matched_any {
            return Ok(SearchResult {
                snapshot_id: snapshot_id.to_owned(),
                hits: Vec::new(),
                abstained: true,
                abstention_reason: Some(AbstentionReason::NoIndexedTerms),
            });
        }

        let mut hits: Vec<SearchHit> = scored
            .into_iter()
            .map(|(path, hit)| SearchHit {
                path,
                content_sha256: hit.hash,
                language: hit.language,
                score_micros: (hit.score.max(0.0) * 1_000_000.0).round() as i64,
                anchor_line: hit.anchor_line,
                matched_terms: hit.terms.into_iter().collect(),
                matched_symbols: hit.symbols.into_iter().collect(),
            })
            .filter(|hit| hit.score_micros >= query.min_score_micros)
            .collect();
        hits.sort_by(|left, right| {
            right
                .score_micros
                .cmp(&left.score_micros)
                .then_with(|| left.path.cmp(&right.path))
        });
        hits.truncate(query.limit);
        let abstained = hits.is_empty();
        Ok(SearchResult {
            snapshot_id: snapshot_id.to_owned(),
            hits,
            abstained,
            abstention_reason: abstained.then_some(AbstentionReason::BelowConfidenceThreshold),
        })
    }

    pub fn search_current(
        &self,
        workspace_root: impl AsRef<Path>,
        query: &SearchQuery,
    ) -> Result<SearchResult, RepoError> {
        let indexed = self.verified_current_snapshot_id(workspace_root)?;
        self.search(&indexed, query)
    }

    /// Returns the exact indexed snapshot for the current workspace without
    /// forcing a lexical query merely to establish snapshot freshness.
    /// Context Economy uses this so deterministic exact evidence can terminate
    /// discovery before a broader retrieval family is invoked.
    pub fn verified_current_snapshot_id(
        &self,
        workspace_root: impl AsRef<Path>,
    ) -> Result<String, RepoError> {
        let current = snapshot_identity(&WorkspaceSnapshot::capture(
            workspace_root.as_ref(),
            &SnapshotPolicy::default(),
        )?)?;
        let indexed = self
            .current_snapshot_id(&current.repo_id)?
            .ok_or_else(|| RepoError::UnknownSnapshot(current.snapshot_id.clone()))?;
        if indexed != current.snapshot_id {
            return Err(RepoError::StaleIndex {
                indexed,
                current: current.snapshot_id,
            });
        }
        self.ensure_snapshot(&indexed)?;
        Ok(indexed)
    }

    pub fn file(&self, snapshot_id: &str, path: &str) -> Result<Option<IndexedFile>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        validate_relative(path)?;
        let row = self
            .connection
            .query_row(
                "SELECT path,content_sha256,byte_len,line_count,language,file_kind,parser_key,is_test FROM snapshot_files WHERE snapshot_id=? AND path=?",
                params![snapshot_id, path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((path, content_sha256, byte_len, line_count, language, kind, parser_key, is_test)) =
            row
        else {
            return Ok(None);
        };
        let byte_len = u64::try_from(byte_len).map_err(|_| {
            RepoError::Integrity(format!(
                "negative or overflowing file byte length for {path}"
            ))
        })?;
        Ok(Some(IndexedFile {
            path,
            content_sha256,
            byte_len,
            line_count,
            language: parse_language(&language),
            kind: parse_file_kind(&kind)?,
            parser_key,
            is_test: is_test != 0,
        }))
    }

    pub fn symbols(&self, snapshot_id: &str, name: &str) -> Result<Vec<SymbolRecord>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut statement = self.connection.prepare(
            "SELECT f.path,f.content_sha256,s.local_id,s.name,s.container,s.kind,s.start_byte,s.end_byte,s.start_line,s.end_line,s.signature FROM snapshot_files f JOIN content_symbols s ON s.content_sha256=f.content_sha256 AND s.parser_key=f.parser_key WHERE f.snapshot_id=? AND lower(s.name)=lower(?) ORDER BY f.path,s.start_byte"
        )?;
        let rows = statement.query_map(params![snapshot_id, name], |row| {
            let path: String = row.get(0)?;
            let hash: String = row.get(1)?;
            let local: String = row.get(2)?;
            Ok(SymbolRecord {
                symbol_id: symbol_id(&path, &hash, &local),
                path,
                name: row.get(3)?,
                container: row.get(4)?,
                kind: parse_symbol_kind(&row.get::<_, String>(5)?),
                start_byte: row.get(6)?,
                end_byte: row.get(7)?,
                start_line: row.get(8)?,
                end_line: row.get(9)?,
                signature: row.get(10)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RepoError::from)
    }

    /// Resolves an exactly named definition, optionally qualified as
    /// `Container::name`. Returns every definition that matches the request so
    /// callers can fail closed on genuine ambiguity instead of guessing.
    pub fn definitions(
        &self,
        snapshot_id: &str,
        qualified_name: &str,
    ) -> Result<Vec<SymbolRecord>, RepoError> {
        let (container, name) = split_qualified_name(qualified_name);
        if name.is_empty() {
            return Ok(Vec::new());
        }
        let mut records = self.symbols(snapshot_id, name)?;
        if let Some(container) = container {
            records.retain(|record| {
                record
                    .container
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(container))
            });
        }
        Ok(records)
    }

    pub fn dependencies(
        &self,
        snapshot_id: &str,
        path: &str,
    ) -> Result<Vec<DependencyEdge>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut statement = self.connection.prepare(
            "SELECT f.content_sha256,l.source_local_id,l.kind,l.target_name,l.line FROM snapshot_files f JOIN content_links l ON l.content_sha256=f.content_sha256 AND l.parser_key=f.parser_key WHERE f.snapshot_id=? AND f.path=? ORDER BY l.line,l.kind,l.target_name"
        )?;
        let rows = statement.query_map(params![snapshot_id, path], |row| {
            let hash: String = row.get(0)?;
            let source_local: Option<String> = row.get(1)?;
            let target: String = row.get(3)?;
            Ok(DependencyEdge {
                source_path: path.to_owned(),
                source_symbol_id: source_local
                    .as_deref()
                    .map(|local| symbol_id(path, &hash, local)),
                kind: parse_edge_kind(&row.get::<_, String>(2)?),
                target_symbol_id: resolve_symbol_id(&self.connection, snapshot_id, &target)
                    .ok()
                    .flatten(),
                target_name: target,
                line: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RepoError::from)
    }

    pub fn tests_for(
        &self,
        snapshot_id: &str,
        target_path: &str,
    ) -> Result<Vec<TestAssociation>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut statement = self.connection.prepare(
            "SELECT test_path,target_path,target_symbol_id,target_symbol_name,confidence_milli FROM test_links WHERE snapshot_id=? AND target_path=? ORDER BY confidence_milli DESC,test_path"
        )?;
        let rows = statement.query_map(params![snapshot_id, target_path], |row| {
            Ok(TestAssociation {
                test_path: row.get(0)?,
                target_path: row.get(1)?,
                target_symbol_id: row.get(2)?,
                target_symbol_name: row.get(3)?,
                confidence_milli: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RepoError::from)
    }

    pub fn git_history(
        &self,
        snapshot_id: &str,
        path: Option<&str>,
    ) -> Result<Vec<GitCommitView>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let sql = if path.is_some() {
            "SELECT DISTINCT c.commit_hash,c.unix_time FROM git_commits c JOIN git_changes g ON g.snapshot_id=c.snapshot_id AND g.commit_hash=c.commit_hash WHERE c.snapshot_id=? AND g.path=? ORDER BY c.ordinal"
        } else {
            "SELECT commit_hash,unix_time FROM git_commits WHERE snapshot_id=? ORDER BY ordinal"
        };
        let mut statement = self.connection.prepare(sql)?;
        let mut output = Vec::new();
        let mut rows = if let Some(path) = path {
            statement.query(params![snapshot_id, path])?
        } else {
            statement.query([snapshot_id])?
        };
        while let Some(row) = rows.next()? {
            let commit: String = row.get(0)?;
            let unix_time: i64 = row.get(1)?;
            let mut changes = self.connection.prepare(
                "SELECT path FROM git_changes WHERE snapshot_id=? AND commit_hash=? ORDER BY path",
            )?;
            let changed_paths = changes
                .query_map(params![snapshot_id, commit], |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            output.push(GitCommitView {
                commit,
                unix_time,
                changed_paths,
            });
        }
        Ok(output)
    }

    pub fn cochanges(
        &self,
        snapshot_id: &str,
        path: &str,
    ) -> Result<Vec<CoChangeRecord>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut statement = self.connection.prepare(
            "SELECT path_a,path_b,count FROM cochanges WHERE snapshot_id=? AND (path_a=? OR path_b=?) ORDER BY count DESC,path_a,path_b"
        )?;
        let rows = statement.query_map(params![snapshot_id, path, path], |row| {
            Ok(CoChangeRecord {
                path_a: row.get(0)?,
                path_b: row.get(1)?,
                count: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RepoError::from)
    }

    pub fn replace_semantic_anchors(
        &mut self,
        snapshot_id: &str,
        anchors: &[SemanticAnchor],
    ) -> Result<Vec<SemanticLink>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM semantic_links WHERE snapshot_id=?",
            [snapshot_id],
        )?;
        let mut output = Vec::new();
        for anchor in anchors {
            let query = SearchQuery {
                text: anchor.text.clone(),
                limit: 5,
                min_score_micros: 100_000,
            };
            let terms = tokenize(&query.text, 256)?;
            for (path, score) in search_paths_tx(&tx, snapshot_id, &terms, 5)? {
                tx.execute("INSERT INTO semantic_links(snapshot_id,semantic_kind,semantic_id,target_path,target_symbol_id,score_micros) VALUES(?,?,?,?,NULL,?)", params![snapshot_id, anchor.kind, anchor.id, path, score])?;
                output.push(SemanticLink {
                    semantic_kind: anchor.kind.clone(),
                    semantic_id: anchor.id.clone(),
                    target_path: path,
                    target_symbol_id: None,
                    score_micros: score,
                });
            }
        }
        tx.commit()?;
        Ok(output)
    }

    pub fn semantic_links(
        &self,
        snapshot_id: &str,
        semantic_id: &str,
    ) -> Result<Vec<SemanticLink>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut statement = self.connection.prepare("SELECT semantic_kind,semantic_id,target_path,target_symbol_id,score_micros FROM semantic_links WHERE snapshot_id=? AND semantic_id=? ORDER BY score_micros DESC,target_path")?;
        let rows = statement.query_map(params![snapshot_id, semantic_id], |row| {
            Ok(SemanticLink {
                semantic_kind: row.get(0)?,
                semantic_id: row.get(1)?,
                target_path: row.get(2)?,
                target_symbol_id: row.get(3)?,
                score_micros: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(RepoError::from)
    }

    pub fn replace_runtime_observations(
        &mut self,
        snapshot_id: &str,
        observations: &[RuntimeObservation],
    ) -> Result<Vec<RuntimeLink>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM runtime_links WHERE snapshot_id=?",
            [snapshot_id],
        )?;
        let mut output = Vec::new();
        for observation in observations {
            validate_relative(&observation.path)?;
            let hash = tx
                .query_row(
                    "SELECT content_sha256 FROM snapshot_files WHERE snapshot_id=? AND path=?",
                    params![snapshot_id, observation.path],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();
            if hash.is_none() {
                continue;
            }
            tx.execute("INSERT INTO runtime_links(snapshot_id,observation_id,path,line,summary,content_sha256) VALUES(?,?,?,?,?,?)", params![snapshot_id, observation.observation_id, observation.path, observation.line, observation.summary, hash])?;
            output.push(RuntimeLink {
                observation_id: observation.observation_id.clone(),
                path: observation.path.clone(),
                line: observation.line,
                summary: observation.summary.clone(),
                content_sha256: hash,
            });
        }
        tx.commit()?;
        Ok(output)
    }

    pub fn impact(&self, snapshot_id: &str, path: &str) -> Result<Vec<ImpactCandidate>, RepoError> {
        self.ensure_snapshot(snapshot_id)?;
        let mut scores: BTreeMap<String, (u32, BTreeSet<String>)> = BTreeMap::new();
        for test in self.tests_for(snapshot_id, path)? {
            add_impact(
                &mut scores,
                &test.test_path,
                test.confidence_milli.into(),
                "test association",
            );
        }
        for pair in self.cochanges(snapshot_id, path)? {
            let other = if pair.path_a == path {
                pair.path_b
            } else {
                pair.path_a
            };
            add_impact(
                &mut scores,
                &other,
                pair.count.saturating_mul(100).min(700),
                "git co-change",
            );
        }
        let basename = Path::new(path)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !basename.is_empty() {
            let query = SearchQuery {
                text: basename,
                limit: 20,
                min_score_micros: 100_000,
            };
            for hit in self.search(snapshot_id, &query)?.hits {
                if hit.path != path {
                    add_impact(&mut scores, &hit.path, 300, "lexical/reference proximity");
                }
            }
        }
        let mut output: Vec<_> = scores
            .into_iter()
            .map(|(path, (score, reasons))| ImpactCandidate {
                path,
                reason: reasons.into_iter().collect::<Vec<_>>().join(", "),
                score_milli: score.min(1000),
            })
            .collect();
        output.sort_by(|left, right| {
            right
                .score_milli
                .cmp(&left.score_milli)
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(output)
    }

    pub fn evaluate(
        &self,
        snapshot_id: &str,
        cases: &[RetrievalCase],
        limit: usize,
    ) -> Result<RetrievalMetrics, RepoError> {
        let mut total = 0_usize;
        let mut found = 0_usize;
        for case in cases {
            total += case.relevant_paths.len();
            let result = self.search(
                snapshot_id,
                &SearchQuery {
                    text: case.query.clone(),
                    limit,
                    min_score_micros: 100_000,
                },
            )?;
            let returned: BTreeSet<_> = result.hits.into_iter().map(|hit| hit.path).collect();
            found += case
                .relevant_paths
                .iter()
                .filter(|path| returned.contains(*path))
                .count();
        }
        let recall = found
            .saturating_mul(1000)
            .checked_div(total)
            .map_or(1000, |value| u16::try_from(value).unwrap_or(1000));
        Ok(RetrievalMetrics {
            cases: cases.len(),
            relevant_total: total,
            relevant_found: found,
            recall_milli: recall,
        })
    }

    pub fn current_snapshot_id(&self, repo_id: &str) -> Result<Option<String>, RepoError> {
        self.connection
            .query_row(
                "SELECT snapshot_id FROM current_snapshots WHERE repo_id=?",
                [repo_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(RepoError::from)
    }

    fn ensure_snapshot(&self, snapshot_id: &str) -> Result<(), RepoError> {
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM snapshots WHERE snapshot_id=?)",
            [snapshot_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(RepoError::UnknownSnapshot(snapshot_id.to_owned()));
        }
        Ok(())
    }

    fn report_existing(
        &self,
        snapshot: RepoSnapshotIdentity,
        already_current: bool,
    ) -> Result<IndexBuildReport, RepoError> {
        let count = |sql: &str| -> Result<usize, RepoError> {
            let value: i64 = self
                .connection
                .query_row(sql, [&snapshot.snapshot_id], |row| row.get(0))?;
            nonnegative_usize(value, "repository count")
        };
        let files_seen = count("SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=?")?;
        let text_files =
            count("SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='text'")?;
        let binary_files = count(
            "SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='binary'",
        )?;
        let oversized_files = count(
            "SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='oversized'",
        )?;
        let symlinks = count(
            "SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='symlink'",
        )?;
        let symbols = count(
            "SELECT COUNT(*) FROM snapshot_files f JOIN content_symbols s ON s.content_sha256=f.content_sha256 AND s.parser_key=f.parser_key WHERE f.snapshot_id=?",
        )?;
        let dependency_edges = count(
            "SELECT COUNT(*) FROM snapshot_files f JOIN content_links l ON l.content_sha256=f.content_sha256 AND l.parser_key=f.parser_key WHERE f.snapshot_id=?",
        )?;
        let test_associations = count("SELECT COUNT(*) FROM test_links WHERE snapshot_id=?")?;
        let git_commits = count("SELECT COUNT(*) FROM git_commits WHERE snapshot_id=?")?;
        let cochange_pairs = count("SELECT COUNT(*) FROM cochanges WHERE snapshot_id=?")?;
        Ok(IndexBuildReport {
            snapshot,
            already_current,
            files_seen,
            text_files,
            binary_files,
            oversized_files,
            symlinks,
            parsed_artifacts: 0,
            reused_artifacts: text_files,
            symbols,
            dependency_edges,
            test_associations,
            git_commits,
            cochange_pairs,
        })
    }
}

#[derive(Clone)]
struct PreparedFile {
    path: String,
    byte_len: u64,
    language: LanguageKind,
    kind: FileKind,
    is_test: bool,
    content_sha256: Option<String>,
    parser_key: Option<String>,
    text: Option<String>,
}

impl PreparedFile {
    fn unindexed(
        path: String,
        byte_len: u64,
        language: LanguageKind,
        kind: FileKind,
        is_test: bool,
    ) -> Self {
        Self {
            path,
            byte_len,
            language,
            kind,
            is_test,
            content_sha256: None,
            parser_key: None,
            text: None,
        }
    }
}

struct HitAccumulator {
    hash: String,
    language: LanguageKind,
    score: f64,
    anchor_line: Option<u32>,
    terms: BTreeSet<String>,
    symbols: BTreeSet<String>,
}

impl HitAccumulator {
    fn new(hash: String, language: LanguageKind, line: u32) -> Self {
        Self {
            hash,
            language,
            score: 0.0,
            anchor_line: Some(line),
            terms: BTreeSet::new(),
            symbols: BTreeSet::new(),
        }
    }
}

struct GitView {
    commits: Vec<GitCommitView>,
    cochanges: Vec<CoChangeRecord>,
}

/// Splits `Container::name` into its optional container and its final segment.
/// Longer paths keep only the immediately enclosing segment, which is what the
/// index records.
fn split_qualified_name(qualified_name: &str) -> (Option<&str>, &str) {
    let trimmed = qualified_name.trim();
    match trimmed.rsplit_once("::") {
        Some((container, name)) => {
            let container = container.trim().rsplit("::").next().unwrap_or("").trim();
            let container = (!container.is_empty()).then_some(container);
            (container, name.trim())
        }
        None => (None, trimmed),
    }
}

fn initialize_schema(connection: &Connection) -> Result<(), RepoError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 0 {
        connection.execute_batch(
            "BEGIN;
             CREATE TABLE snapshots(snapshot_id TEXT PRIMARY KEY,repo_id TEXT NOT NULL,repo_root TEXT NOT NULL,head_commit TEXT NOT NULL,dirty_sha TEXT NOT NULL,untracked_sha TEXT NOT NULL,submodule_sha TEXT NOT NULL,created_at INTEGER NOT NULL DEFAULT(unixepoch()));
             CREATE INDEX snapshots_repo_created ON snapshots(repo_id,created_at DESC);
             CREATE TABLE current_snapshots(repo_id TEXT PRIMARY KEY,snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE);
             CREATE TABLE content_artifacts(content_sha256 TEXT NOT NULL,parser_key TEXT NOT NULL,line_count INTEGER NOT NULL,token_count INTEGER NOT NULL,parse_had_error INTEGER NOT NULL,PRIMARY KEY(content_sha256,parser_key));
             CREATE TABLE content_terms(content_sha256 TEXT NOT NULL,parser_key TEXT NOT NULL,term TEXT NOT NULL,tf INTEGER NOT NULL,first_line INTEGER NOT NULL,PRIMARY KEY(content_sha256,parser_key,term),FOREIGN KEY(content_sha256,parser_key) REFERENCES content_artifacts(content_sha256,parser_key) ON DELETE CASCADE);
             CREATE INDEX terms_term ON content_terms(term);
             CREATE TABLE content_symbols(content_sha256 TEXT NOT NULL,parser_key TEXT NOT NULL,local_id TEXT NOT NULL,name TEXT NOT NULL,kind TEXT NOT NULL,start_byte INTEGER NOT NULL,end_byte INTEGER NOT NULL,start_line INTEGER NOT NULL,end_line INTEGER NOT NULL,signature TEXT NOT NULL,PRIMARY KEY(content_sha256,parser_key,local_id),FOREIGN KEY(content_sha256,parser_key) REFERENCES content_artifacts(content_sha256,parser_key) ON DELETE CASCADE);
             CREATE INDEX symbols_name ON content_symbols(name);
             CREATE TABLE content_links(content_sha256 TEXT NOT NULL,parser_key TEXT NOT NULL,source_local_id TEXT,kind TEXT NOT NULL,target_name TEXT NOT NULL,line INTEGER NOT NULL,FOREIGN KEY(content_sha256,parser_key) REFERENCES content_artifacts(content_sha256,parser_key) ON DELETE CASCADE);
             CREATE INDEX links_target ON content_links(target_name);
             CREATE TABLE snapshot_files(snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,path TEXT NOT NULL,content_sha256 TEXT,parser_key TEXT,byte_len INTEGER NOT NULL,line_count INTEGER NOT NULL,language TEXT NOT NULL,file_kind TEXT NOT NULL,is_test INTEGER NOT NULL,PRIMARY KEY(snapshot_id,path));
             CREATE INDEX snapshot_files_content ON snapshot_files(content_sha256,parser_key);
             CREATE TABLE git_commits(snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,commit_hash TEXT NOT NULL,unix_time INTEGER NOT NULL,ordinal INTEGER NOT NULL,PRIMARY KEY(snapshot_id,commit_hash));
             CREATE TABLE git_changes(snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,commit_hash TEXT NOT NULL,path TEXT NOT NULL,PRIMARY KEY(snapshot_id,commit_hash,path));
             CREATE INDEX git_changes_path ON git_changes(snapshot_id,path);
             CREATE TABLE cochanges(snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,path_a TEXT NOT NULL,path_b TEXT NOT NULL,count INTEGER NOT NULL,PRIMARY KEY(snapshot_id,path_a,path_b));
             CREATE TABLE test_links(snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,test_path TEXT NOT NULL,target_path TEXT NOT NULL,target_symbol_id TEXT,target_symbol_name TEXT,confidence_milli INTEGER NOT NULL,PRIMARY KEY(snapshot_id,test_path,target_path,target_symbol_name));
             CREATE TABLE semantic_links(snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,semantic_kind TEXT NOT NULL,semantic_id TEXT NOT NULL,target_path TEXT NOT NULL,target_symbol_id TEXT,score_micros INTEGER NOT NULL,PRIMARY KEY(snapshot_id,semantic_kind,semantic_id,target_path));
             CREATE TABLE runtime_links(snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,observation_id TEXT NOT NULL,path TEXT NOT NULL,line INTEGER,summary TEXT NOT NULL,content_sha256 TEXT,PRIMARY KEY(snapshot_id,observation_id));
             PRAGMA user_version=1;
             COMMIT;"
        )?;
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 1 {
        ri2::migrate_v1_to_v2(connection)?;
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 2 {
        migrate_v2_to_v3(connection)?;
    } else if version != INDEX_SCHEMA_VERSION {
        return Err(RepoError::UnsupportedIndexVersion(version));
    }
    Ok(())
}

/// v3 records the lexically enclosing definition scope of every symbol so exact
/// `Container::name` definition retrieval can resolve without a second index.
/// Existing artifacts are re-parsed because the extraction query version changed.
fn migrate_v2_to_v3(connection: &Connection) -> Result<(), RepoError> {
    connection.execute_batch(
        "BEGIN;
         ALTER TABLE content_symbols ADD COLUMN container TEXT;
         PRAGMA user_version=3;
         COMMIT;",
    )?;
    Ok(())
}

fn clear_snapshot_materialized_views(
    transaction: &Transaction<'_>,
    snapshot_id: &str,
    producer_rebuild: bool,
) -> Result<(), RepoError> {
    for table in [
        "runtime_links",
        "semantic_links",
        "test_links",
        "cochanges",
        "git_changes",
        "git_commits",
        "snapshot_files",
    ] {
        transaction.execute(
            &format!("DELETE FROM {table} WHERE snapshot_id=?"),
            [snapshot_id],
        )?;
    }
    if producer_rebuild {
        transaction.execute(
            "DELETE FROM ri2_symbol_continuity WHERE from_snapshot=? OR to_snapshot=?",
            params![snapshot_id, snapshot_id],
        )?;
    }
    Ok(())
}

fn insert_snapshot(tx: &Transaction<'_>, snapshot: &RepoSnapshotIdentity) -> Result<(), RepoError> {
    tx.execute("INSERT OR IGNORE INTO snapshots(snapshot_id,repo_id,repo_root,head_commit,dirty_sha,untracked_sha,submodule_sha) VALUES(?,?,?,?,?,?,?)", params![snapshot.snapshot_id,snapshot.repo_id,snapshot.repo_root.to_string_lossy(),snapshot.head_commit,snapshot.dirty_tracked_diff_sha256,snapshot.untracked_content_sha256,snapshot.submodule_state_sha256])?;
    Ok(())
}

fn artifact_exists(tx: &Transaction<'_>, hash: &str, key: &str) -> Result<bool, RepoError> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM content_artifacts WHERE content_sha256=? AND parser_key=?)",
        params![hash, key],
        |row| row.get(0),
    )?)
}

fn insert_artifact(
    tx: &Transaction<'_>,
    hash: &str,
    key: &str,
    artifact: &syntax::ParsedArtifact,
) -> Result<(), RepoError> {
    tx.execute("INSERT INTO content_artifacts(content_sha256,parser_key,line_count,token_count,parse_had_error) VALUES(?,?,?,?,?)", params![hash,key,artifact.line_count,artifact.token_count,if artifact.parse_had_error {1}else{0}])?;
    for term in &artifact.terms {
        tx.execute("INSERT INTO content_terms(content_sha256,parser_key,term,tf,first_line) VALUES(?,?,?,?,?)", params![hash,key,term.term,term.tf,term.first_line])?;
    }
    for symbol in &artifact.symbols {
        tx.execute("INSERT INTO content_symbols(content_sha256,parser_key,local_id,name,container,kind,start_byte,end_byte,start_line,end_line,signature) VALUES(?,?,?,?,?,?,?,?,?,?,?)", params![hash,key,symbol.local_id,symbol.name,symbol.container,symbol.kind.as_str(),symbol.start_byte,symbol.end_byte,symbol.start_line,symbol.end_line,symbol.signature])?;
    }
    for link in &artifact.links {
        tx.execute("INSERT INTO content_links(content_sha256,parser_key,source_local_id,kind,target_name,line) VALUES(?,?,?,?,?,?)", params![hash,key,link.source_local_id,link.kind.as_str(),link.target_name,link.line])?;
    }
    Ok(())
}

fn artifact_line_count(tx: &Transaction<'_>, hash: &str, key: &str) -> Result<u32, RepoError> {
    Ok(tx.query_row(
        "SELECT line_count FROM content_artifacts WHERE content_sha256=? AND parser_key=?",
        params![hash, key],
        |row| row.get(0),
    )?)
}
fn artifact_symbol_count(tx: &Transaction<'_>, hash: &str, key: &str) -> Result<usize, RepoError> {
    let value: i64 = tx.query_row(
        "SELECT COUNT(*) FROM content_symbols WHERE content_sha256=? AND parser_key=?",
        params![hash, key],
        |row| row.get(0),
    )?;
    nonnegative_usize(value, "artifact symbol count")
}
fn artifact_link_count(tx: &Transaction<'_>, hash: &str, key: &str) -> Result<usize, RepoError> {
    let value: i64 = tx.query_row(
        "SELECT COUNT(*) FROM content_links WHERE content_sha256=? AND parser_key=?",
        params![hash, key],
        |row| row.get(0),
    )?;
    nonnegative_usize(value, "artifact link count")
}

fn rebuild_test_links(
    tx: &Transaction<'_>,
    snapshot_id: &str,
    per_test_limit: usize,
) -> Result<usize, RepoError> {
    tx.execute("DELETE FROM test_links WHERE snapshot_id=?", [snapshot_id])?;
    let mut tests = tx.prepare("SELECT path,content_sha256,parser_key FROM snapshot_files WHERE snapshot_id=? AND is_test=1 AND content_sha256 IS NOT NULL AND parser_key IS NOT NULL ORDER BY path")?;
    let rows = tests.query_map([snapshot_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut inserted = 0;
    for row in rows {
        let (test_path, hash, key) = row?;
        let mut symbols=tx.prepare("SELECT DISTINCT f.path,f.content_sha256,s.local_id,s.name FROM content_terms t JOIN snapshot_files f ON f.snapshot_id=? JOIN content_symbols s ON s.content_sha256=f.content_sha256 AND s.parser_key=f.parser_key WHERE t.content_sha256=? AND t.parser_key=? AND t.term=lower(s.name) AND f.path<>? ORDER BY f.path,s.name LIMIT ?")?;
        let candidates = symbols.query_map(
            params![
                snapshot_id,
                hash,
                key,
                test_path,
                i64::try_from(per_test_limit).unwrap_or(100)
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;
        for candidate in candidates {
            let (target_path, target_hash, local, name) = candidate?;
            let id = symbol_id(&target_path, &target_hash, &local);
            tx.execute("INSERT OR IGNORE INTO test_links(snapshot_id,test_path,target_path,target_symbol_id,target_symbol_name,confidence_milli) VALUES(?,?,?,?,?,800)",params![snapshot_id,test_path,target_path,id,name])?;
            inserted += 1;
        }
    }
    Ok(inserted)
}

fn collect_git_view(repo: &Path, policy: &IndexPolicy) -> Result<GitView, RepoError> {
    let limit = policy.max_git_commits.to_string();
    let bytes = run_git(
        repo,
        [
            "log",
            "-z",
            "--format=%x1e%H%x1f%ct%x1f",
            "--name-only",
            "--no-renames",
            "-n",
            &limit,
            "--",
        ],
        policy,
        "git log",
    )?;
    let mut commits = Vec::new();
    let mut pairs: BTreeMap<(String, String), u32> = BTreeMap::new();
    for record in bytes
        .split(|byte| *byte == 0x1e)
        .filter(|record| !record.is_empty())
    {
        let fields: Vec<&[u8]> = record.splitn(3, |byte| *byte == 0x1f).collect();
        if fields.len() != 3 {
            continue;
        }
        let hash = std::str::from_utf8(fields[0])
            .map_err(|_| RepoError::Git("non-UTF8 git commit id".to_owned()))?
            .trim()
            .to_owned();
        let time = std::str::from_utf8(fields[1])
            .map_err(|_| RepoError::Git("non-UTF8 git timestamp".to_owned()))?
            .trim()
            .parse::<i64>()
            .map_err(|_| RepoError::Git("invalid git timestamp".to_owned()))?;
        let mut paths = Vec::new();
        for raw in fields[2]
            .split(|byte| *byte == 0)
            .filter(|value| !value.is_empty())
        {
            let raw = raw.strip_prefix(b"\n").unwrap_or(raw);
            if raw.is_empty() {
                continue;
            }
            let path = std::str::from_utf8(raw)
                .map_err(|_| RepoError::NonUtf8Path)?
                .to_owned();
            validate_relative(&path)?;
            paths.push(path);
        }
        paths.sort();
        paths.dedup();
        if paths.len() <= policy.max_cochange_files_per_commit {
            for i in 0..paths.len() {
                for j in (i + 1)..paths.len() {
                    *pairs
                        .entry((paths[i].clone(), paths[j].clone()))
                        .or_default() += 1;
                }
            }
        }
        commits.push(GitCommitView {
            commit: hash,
            unix_time: time,
            changed_paths: paths,
        });
    }
    let cochanges = pairs
        .into_iter()
        .map(|((path_a, path_b), count)| CoChangeRecord {
            path_a,
            path_b,
            count,
        })
        .collect();
    Ok(GitView { commits, cochanges })
}

fn insert_git_view(
    tx: &Transaction<'_>,
    snapshot_id: &str,
    view: &GitView,
) -> Result<(), RepoError> {
    for (ordinal, commit) in view.commits.iter().enumerate() {
        tx.execute(
            "INSERT INTO git_commits(snapshot_id,commit_hash,unix_time,ordinal) VALUES(?,?,?,?)",
            params![
                snapshot_id,
                commit.commit,
                commit.unix_time,
                i64::try_from(ordinal).unwrap_or(i64::MAX)
            ],
        )?;
        for path in &commit.changed_paths {
            tx.execute(
                "INSERT INTO git_changes(snapshot_id,commit_hash,path) VALUES(?,?,?)",
                params![snapshot_id, commit.commit, path],
            )?;
        }
    }
    for pair in &view.cochanges {
        tx.execute(
            "INSERT INTO cochanges(snapshot_id,path_a,path_b,count) VALUES(?,?,?,?)",
            params![snapshot_id, pair.path_a, pair.path_b, pair.count],
        )?;
    }
    Ok(())
}

fn list_repository_files(repo: &Path, policy: &IndexPolicy) -> Result<Vec<String>, RepoError> {
    let bytes = run_git(
        repo,
        [
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        policy,
        "git ls-files",
    )?;
    let mut paths = Vec::new();
    for raw in bytes.split(|byte| *byte == 0).filter(|v| !v.is_empty()) {
        let path = std::str::from_utf8(raw)
            .map_err(|_| RepoError::NonUtf8Path)?
            .to_owned();
        validate_relative(&path)?;
        paths.push(path);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn run_git<const N: usize>(
    repo: &Path,
    args: [&str; N],
    policy: &IndexPolicy,
    operation: &'static str,
) -> Result<Vec<u8>, RepoError> {
    let execution =
        ExecutionPolicy::trusted_workspace(repo, policy.git_timeout, policy.max_git_output_bytes)?;
    let result = LocalProcessExecutor.execute(
        &execution,
        CommandSpec::new("git", repo, SideEffectClass::PureRead).args(args),
    )?;
    if result.stdout.truncated {
        return Err(RepoError::OutputTooLarge {
            operation,
            bytes: result.stdout.total_bytes,
        });
    }
    if !result.success {
        return Err(RepoError::Git(
            String::from_utf8_lossy(&result.stderr.preview)
                .trim()
                .to_owned(),
        ));
    }
    Ok(result.stdout.preview)
}

fn snapshot_identity(snapshot: &WorkspaceSnapshot) -> Result<RepoSnapshotIdentity, RepoError> {
    let mut untracked = Sha256::new();
    untracked.update(b"aer-repo-untracked-v1\0");
    let mut files = snapshot.untracked_files.iter().collect::<Vec<_>>();
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    for file in files {
        let path = path_string(&file.relative_path)?;
        untracked.update(path.as_bytes());
        untracked.update(b"\0");
        untracked.update(file.sha256.as_bytes());
        untracked.update(b"\0");
    }
    let untracked_content_sha256 = sha256_digest(untracked.finalize().as_ref());
    let mut identity = Sha256::new();
    identity.update(b"aer-repo-snapshot-v1\0");
    for part in [
        &snapshot.identity.repo_id,
        &snapshot.identity.head_commit,
        &snapshot.identity.dirty_tracked_diff_sha256,
        &untracked_content_sha256,
        &snapshot.identity.submodule_state_sha256,
    ] {
        identity.update(part.as_bytes());
        identity.update(b"\0");
    }
    Ok(RepoSnapshotIdentity {
        snapshot_id: format!(
            "repo-snapshot:{}",
            sha256_digest(identity.finalize().as_ref())
        ),
        repo_id: snapshot.identity.repo_id.clone(),
        repo_root: snapshot.identity.repo_root.clone(),
        head_commit: snapshot.identity.head_commit.clone(),
        dirty_tracked_diff_sha256: snapshot.identity.dirty_tracked_diff_sha256.clone(),
        untracked_content_sha256,
        submodule_state_sha256: snapshot.identity.submodule_state_sha256.clone(),
    })
}

fn nonnegative_usize(value: i64, label: &str) -> Result<usize, RepoError> {
    usize::try_from(value)
        .map_err(|_| RepoError::Integrity(format!("negative or overflowing {label}: {value}")))
}

fn validate_query(query: &SearchQuery, policy: &IndexPolicy) -> Result<(), RepoError> {
    if query.text.len() > policy.max_query_bytes {
        return Err(RepoError::QueryTooLarge(query.text.len()));
    }
    if query.min_score_micros < 0 {
        return Err(RepoError::Integrity(
            "query score threshold cannot be negative".to_owned(),
        ));
    }
    if query.limit == 0 || query.limit > policy.max_results {
        return Err(RepoError::ResultLimitExceeded(query.limit));
    }
    Ok(())
}
fn validate_relative(path: &str) -> Result<(), RepoError> {
    let p = Path::new(path);
    if p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(RepoError::InvalidRelativePath(path.to_owned()));
    }
    Ok(())
}
fn path_string(path: &Path) -> Result<String, RepoError> {
    path.to_str()
        .map(|v| v.replace('\\', "/"))
        .ok_or(RepoError::NonUtf8Path)
}
fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_digest(Sha256::digest(bytes).as_ref()))
}
fn sha256_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 15) as usize] as char);
    }
    out
}
fn symbol_id(path: &str, hash: &str, local: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"aer-repo-symbol-v1\0");
    digest.update(path.as_bytes());
    digest.update(b"\0");
    digest.update(hash.as_bytes());
    digest.update(b"\0");
    digest.update(local.as_bytes());
    format!("symbol:{}", sha256_digest(digest.finalize().as_ref()))
}
fn resolve_symbol_id(
    connection: &Connection,
    snapshot: &str,
    name: &str,
) -> Result<Option<String>, RepoError> {
    let row:Option<(String,String,String)>=connection.query_row("SELECT f.path,f.content_sha256,s.local_id FROM snapshot_files f JOIN content_symbols s ON s.content_sha256=f.content_sha256 AND s.parser_key=f.parser_key WHERE f.snapshot_id=? AND lower(s.name)=lower(?) ORDER BY f.path,s.start_byte LIMIT 1",params![snapshot,name],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    Ok(row.map(|(path, hash, local)| symbol_id(&path, &hash, &local)))
}
fn parse_file_kind(value: &str) -> Result<FileKind, RepoError> {
    match value {
        "text" => Ok(FileKind::Text),
        "binary" => Ok(FileKind::Binary),
        "oversized" => Ok(FileKind::Oversized),
        "symlink" => Ok(FileKind::Symlink),
        _ => Err(RepoError::Integrity(format!(
            "unknown indexed file kind: {value}"
        ))),
    }
}

fn parse_language(value: &str) -> LanguageKind {
    match value {
        "rust" => LanguageKind::Rust,
        "python" => LanguageKind::Python,
        "javascript" => LanguageKind::JavaScript,
        "typescript" => LanguageKind::TypeScript,
        "tsx" => LanguageKind::Tsx,
        "json" => LanguageKind::Json,
        "toml" => LanguageKind::Toml,
        "markdown" => LanguageKind::Markdown,
        "shell" => LanguageKind::Shell,
        "yaml" => LanguageKind::Yaml,
        _ => LanguageKind::Other,
    }
}
fn parse_symbol_kind(value: &str) -> SymbolKind {
    match value {
        "function" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "interface" => SymbolKind::Interface,
        "module" => SymbolKind::Module,
        "type_alias" => SymbolKind::TypeAlias,
        "constant" => SymbolKind::Constant,
        "static" => SymbolKind::Static,
        "macro" => SymbolKind::Macro,
        "variable" => SymbolKind::Variable,
        "test" => SymbolKind::Test,
        _ => SymbolKind::Other,
    }
}
fn parse_edge_kind(value: &str) -> EdgeKind {
    match value {
        "imports" => EdgeKind::Imports,
        "calls" => EdgeKind::Calls,
        _ => EdgeKind::References,
    }
}
fn add_impact(
    scores: &mut BTreeMap<String, (u32, BTreeSet<String>)>,
    path: &str,
    score: u32,
    reason: &str,
) {
    let entry = scores.entry(path.to_owned()).or_default();
    entry.0 = entry.0.saturating_add(score).min(1000);
    entry.1.insert(reason.to_owned());
}

fn search_paths_tx(
    tx: &Transaction<'_>,
    snapshot: &str,
    terms: &[String],
    limit: usize,
) -> Result<Vec<(String, i64)>, RepoError> {
    let mut scores: BTreeMap<String, i64> = BTreeMap::new();
    for term in terms {
        let mut statement=tx.prepare("SELECT f.path,t.tf FROM snapshot_files f JOIN content_terms t ON t.content_sha256=f.content_sha256 AND t.parser_key=f.parser_key WHERE f.snapshot_id=? AND t.term=?")?;
        for row in statement.query_map(params![snapshot, term], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (path, tf) = row?;
            *scores.entry(path).or_default() += tf.saturating_mul(1_000_000);
        }
    }
    let mut values: Vec<_> = scores.into_iter().collect();
    values.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    values.truncate(limit);
    Ok(values)
}

fn prune_snapshots(tx: &Transaction<'_>, repo_id: &str, retain: usize) -> Result<(), RepoError> {
    let current: Option<String> = tx
        .query_row(
            "SELECT snapshot_id FROM current_snapshots WHERE repo_id=?",
            [repo_id],
            |row| row.get(0),
        )
        .optional()?;
    let mut statement = tx.prepare(
        "SELECT snapshot_id FROM snapshots WHERE repo_id=? AND snapshot_id<>COALESCE(?, '') ORDER BY created_at DESC,snapshot_id DESC",
    )?;
    let ids = statement
        .query_map(params![repo_id, current], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids.into_iter().skip(retain.saturating_sub(1)) {
        tx.execute("DELETE FROM snapshots WHERE snapshot_id=?", [id])?;
    }
    Ok(())
}
fn garbage_collect_artifacts(tx: &Transaction<'_>) -> Result<(), RepoError> {
    tx.execute("DELETE FROM content_artifacts WHERE NOT EXISTS(SELECT 1 FROM snapshot_files f WHERE f.content_sha256=content_artifacts.content_sha256 AND f.parser_key=content_artifacts.parser_key)",[])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    struct Fixture {
        root: PathBuf,
        index: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let base = std::env::temp_dir().join(format!("aer-repo-{now}-{nonce}"));
            let root = base.join("repo");
            fs::create_dir_all(root.join("src")).unwrap();
            fs::create_dir_all(root.join("tests")).unwrap();
            run(&root, ["init"]);
            run(&root, ["config", "user.email", "aer@example.invalid"]);
            run(&root, ["config", "user.name", "AER Test"]);
            fs::write(
                root.join("src/auth.rs"),
                "pub fn verify_token(token: &str) -> bool { !token.contains(\"expired\") }\n",
            )
            .unwrap();
            fs::write(root.join("src/session.rs"),"use crate::auth::verify_token;\npub fn open(token: &str) -> bool { verify_token(token) }\n").unwrap();
            fs::write(
                root.join("src/math.rs"),
                "pub fn add(a:i32,b:i32)->i32{a+b}\n",
            )
            .unwrap();
            fs::write(
                root.join("tests/auth_test.rs"),
                "#[test]\nfn expired_token_is_rejected(){ assert!(!verify_token(\"expired\")); }\n",
            )
            .unwrap();
            run(&root, ["add", "."]);
            run(&root, ["commit", "-m", "initial"]);
            let index = base.join("index.sqlite");
            Self { root, index }
        }
        fn db(&self) -> RepositoryIndex {
            RepositoryIndex::open(&self.index, IndexPolicy::default()).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Some(base) = self.root.parent() {
                let _ = fs::remove_dir_all(base);
            }
        }
    }
    fn run<const N: usize>(root: &Path, args: [&str; N]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .unwrap();
        assert!(status.success());
    }
    #[test]
    fn retrieval_symbols_and_abstention() {
        let fixture = Fixture::new();
        let mut db = fixture.db();
        let report = db.refresh(&fixture.root).unwrap();
        let result = db
            .search(
                &report.snapshot.snapshot_id,
                &SearchQuery::new("expired token verification"),
            )
            .unwrap();
        assert!(!result.abstained);
        assert!(result.hits.iter().any(|hit| hit.path == "src/auth.rs"));
        assert!(
            !db.symbols(&report.snapshot.snapshot_id, "verify_token")
                .unwrap()
                .is_empty()
        );
        let none = db
            .search(
                &report.snapshot.snapshot_id,
                &SearchQuery::new("qzxwvvnonexistent"),
            )
            .unwrap();
        assert!(none.abstained);
    }
    #[test]
    fn qualified_definitions_resolve_the_exact_defining_span() {
        let fixture = Fixture::new();
        fs::write(
            fixture.root.join("src/capsule.rs"),
            "pub struct Capsule {\n    pub version: u32,\n}\n\npub struct Envelope {\n    pub version: u32,\n}\n\nimpl Capsule {\n    pub fn compile() -> Self {\n        Self { version: 3 }\n    }\n}\n\nimpl Envelope {\n    pub fn compile() -> Self {\n        Self { version: 2 }\n    }\n}\n",
        )
        .unwrap();
        run(&fixture.root, ["add", "."]);
        run(&fixture.root, ["commit", "-m", "capsule"]);
        let mut db = fixture.db();
        let report = db.refresh(&fixture.root).unwrap();
        let snapshot = &report.snapshot.snapshot_id;

        let unqualified = db.definitions(snapshot, "compile").unwrap();
        assert_eq!(unqualified.len(), 2, "bare name is genuinely ambiguous");

        let qualified = db.definitions(snapshot, "Capsule::compile").unwrap();
        assert_eq!(qualified.len(), 1);
        let record = &qualified[0];
        assert_eq!(record.path, "src/capsule.rs");
        assert_eq!(record.container.as_deref(), Some("Capsule"));
        let source = fs::read_to_string(fixture.root.join("src/capsule.rs")).unwrap();
        let defining = source
            .lines()
            .skip((record.start_line - 1) as usize)
            .take((record.end_line - record.start_line + 1) as usize)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(defining.contains("version: 3"));
        assert!(!defining.contains("version: 2"));

        assert!(
            db.definitions(snapshot, "Capsule::missing_symbol")
                .unwrap()
                .is_empty()
        );
        assert!(
            db.definitions(snapshot, "Envelope::compile")
                .unwrap()
                .iter()
                .all(|found| found.container.as_deref() == Some("Envelope"))
        );
    }
    #[test]
    fn stale_index_is_never_reused_and_content_artifacts_are_reused() {
        let fixture = Fixture::new();
        let mut db = fixture.db();
        let first = db.refresh(&fixture.root).unwrap();
        fs::write(
            fixture.root.join("src/math.rs"),
            "pub fn add(a:i32,b:i32)->i32{a+b+1}\n",
        )
        .unwrap();
        assert!(matches!(
            db.search_current(&fixture.root, &SearchQuery::new("verify_token")),
            Err(RepoError::StaleIndex { .. })
        ));
        let second = db.refresh(&fixture.root).unwrap();
        assert_ne!(first.snapshot.snapshot_id, second.snapshot.snapshot_id);
        assert!(second.reused_artifacts >= 3);
    }
    #[test]
    fn test_git_impact_and_baseline_are_measured() {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("src/auth.rs"),"pub fn verify_token(token:&str)->bool{!token.contains(\"expired\")} // expiry validation\n").unwrap();
        fs::write(
            fixture.root.join("tests/auth_test.rs"),
            "#[test]\nfn verify_token_rejects_expired(){ assert!(!verify_token(\"expired\")); }\n",
        )
        .unwrap();
        run(&fixture.root, ["add", "."]);
        run(&fixture.root, ["commit", "-m", "auth behavior"]);
        let mut db = fixture.db();
        let report = db.refresh(&fixture.root).unwrap();
        let tests = db
            .tests_for(&report.snapshot.snapshot_id, "src/auth.rs")
            .unwrap();
        assert!(
            tests
                .iter()
                .any(|test| test.test_path == "tests/auth_test.rs")
        );
        let pairs = db
            .cochanges(&report.snapshot.snapshot_id, "src/auth.rs")
            .unwrap();
        assert!(
            pairs
                .iter()
                .any(|pair| pair.path_a == "tests/auth_test.rs"
                    || pair.path_b == "tests/auth_test.rs")
        );
        let metrics = db
            .evaluate(
                &report.snapshot.snapshot_id,
                &[RetrievalCase {
                    query: "expired token verification".to_owned(),
                    relevant_paths: vec!["src/auth.rs".to_owned(), "tests/auth_test.rs".to_owned()],
                }],
                3,
            )
            .unwrap();
        assert!(metrics.recall_milli >= 1000);
    }
}
