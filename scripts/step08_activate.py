from pathlib import Path
import re


def replace(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"expected activation anchor missing in {path}: {old!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


replace(
    "Cargo.toml",
    '    "crates/aer-core",\n    "crates/aer-cli",',
    '    "crates/aer-core",\n    "crates/aer-repo",\n    "crates/aer-cli",',
)
replace(
    "Cargo.toml",
    'sha2 = "=0.11.0"\nulid = "=3.0.0"',
    'sha2 = "=0.11.0"\n'
    'tree-sitter = "=0.26.11"\n'
    'tree-sitter-javascript = "=0.25.0"\n'
    'tree-sitter-python = "=0.25.0"\n'
    'tree-sitter-rust = "=0.24.2"\n'
    'tree-sitter-typescript = "=0.23.2"\n'
    'ulid = "=3.0.0"',
)
replace(
    "crates/aer-core/Cargo.toml",
    'aer-research = { path = "../aer-research" }',
    'aer-repo = { path = "../aer-repo" }\naer-research = { path = "../aer-research" }',
)
replace(
    "crates/aer-core/src/root.rs",
    'pub use phase1_runtime::*;\npub mod spec;',
    'pub use phase1_runtime::*;\npub mod repository;\npub mod spec;',
)

model = Path("crates/aer-repo/src/model.rs")
text = model.read_text(encoding="utf-8")
text = text.replace("pub min_score_micros: u64", "pub min_score_micros: i64")
text = text.replace("pub score_micros: u64", "pub score_micros: i64")
model.write_text(text, encoding="utf-8")

lib = Path("crates/aer-repo/src/lib.rs")
text = lib.read_text(encoding="utf-8")
text = text.replace('    ffi::OsString,\n', '')
text = text.replace(
    'use aer_workspace::{SnapshotPolicy, WorkspaceIdentity, WorkspaceSnapshot};',
    'use aer_workspace::{SnapshotPolicy, WorkspaceSnapshot};',
)
text = text.replace("path::{Component, Path, PathBuf}", "path::{Component, Path}")

old_hash = '            let hash: Option<String> = tx.query_row("SELECT content_sha256 FROM snapshot_files WHERE snapshot_id=? AND path=?", params![snapshot_id, observation.path], |row| row.get(0)).optional()?;'
new_hash = '''            let hash = tx
                .query_row(
                    "SELECT content_sha256 FROM snapshot_files WHERE snapshot_id=? AND path=?",
                    params![snapshot_id, observation.path],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .flatten();'''
if old_hash not in text:
    raise SystemExit("runtime observation hash anchor missing")
text = text.replace(old_hash, new_hash, 1)

old_fixture = 'let root=std::env::temp_dir().join(format!("aer-repo-{now}-{nonce}"));fs::create_dir_all(root.join("src")).unwrap();fs::create_dir_all(root.join("tests")).unwrap();'
new_fixture = 'let base=std::env::temp_dir().join(format!("aer-repo-{now}-{nonce}"));let root=base.join("repo");fs::create_dir_all(root.join("src")).unwrap();fs::create_dir_all(root.join("tests")).unwrap();'
if old_fixture not in text:
    raise SystemExit("fixture root anchor missing")
text = text.replace(old_fixture, new_fixture, 1)
text = text.replace(
    'let index=root.join(".index.sqlite");Self{root,index}',
    'let index=base.join("index.sqlite");Self{root,index}',
    1,
)
text = text.replace(
    'impl Drop for Fixture{fn drop(&mut self){let _=fs::remove_dir_all(&self.root);}}',
    'impl Drop for Fixture{fn drop(&mut self){if let Some(base)=self.root.parent(){let _=fs::remove_dir_all(base);}}}',
    1,
)
text = text.replace(
    'use std::{fs,process::Command,sync::atomic::{AtomicU64,Ordering},time::{SystemTime,UNIX_EPOCH}};',
    'use std::{fs,path::PathBuf,process::Command,sync::atomic::{AtomicU64,Ordering},time::{SystemTime,UNIX_EPOCH}};',
    1,
)

old_prune = 'fn prune_snapshots(tx:&Transaction<\'_>,repo_id:&str,retain:usize)->Result<(),RepoError>{let mut statement=tx.prepare("SELECT snapshot_id FROM snapshots WHERE repo_id=? ORDER BY created_at DESC,snapshot_id DESC")?;let ids=statement.query_map([repo_id],|row|row.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;for id in ids.into_iter().skip(retain){tx.execute("DELETE FROM snapshots WHERE snapshot_id=?",[id])?;}Ok(())}'
new_prune = '''fn prune_snapshots(
    tx: &Transaction<'_>,
    repo_id: &str,
    retain: usize,
) -> Result<(), RepoError> {
    let current: Option<String> = tx.query_row(
        "SELECT snapshot_id FROM current_snapshots WHERE repo_id=?",
        [repo_id],
        |row| row.get(0),
    ).optional()?;
    let mut statement = tx.prepare(
        "SELECT snapshot_id FROM snapshots WHERE repo_id=? AND snapshot_id<>COALESCE(?, '') ORDER BY created_at DESC,snapshot_id DESC",
    )?;
    let ids = statement.query_map(params![repo_id, current], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in ids.into_iter().skip(retain.saturating_sub(1)) {
        tx.execute("DELETE FROM snapshots WHERE snapshot_id=?", [id])?;
    }
    Ok(())
}'''
if old_prune not in text:
    raise SystemExit("snapshot prune anchor missing")
text = text.replace(old_prune, new_prune, 1)

text = text.replace("let document_count: u64", "let document_count: i64")
text = text.replace("let total_tokens: u64", "let total_tokens: i64")
text = text.replace("let df: u64", "let df: i64")
text = text.replace("row.get::<_, u64>", "row.get::<_, i64>")
text = text.replace("row.get::<_,u64>", "row.get::<_,i64>")
text = text.replace(
    "score_micros: (hit.score.max(0.0) * 1_000_000.0).round() as u64",
    "score_micros: (hit.score.max(0.0) * 1_000_000.0).round() as i64",
)
text = text.replace("BTreeMap<String,u64>", "BTreeMap<String,i64>")
text = text.replace(
    "fn search_paths_tx(tx:&Transaction<'_>,snapshot:&str,terms:&[String],limit:usize)->Result<Vec<(String,u64)>,RepoError>",
    "fn search_paths_tx(tx:&Transaction<'_>,snapshot:&str,terms:&[String],limit:usize)->Result<Vec<(String,i64)>,RepoError>",
    1,
)

start = text.index("    fn report_existing(")
end = text.index("\n}\n\n#[derive(Clone)]", start)
replacement = '''    fn report_existing(
        &self,
        snapshot: RepoSnapshotIdentity,
        already_current: bool,
    ) -> Result<IndexBuildReport, RepoError> {
        let count = |sql: &str| -> Result<usize, RepoError> {
            let value: i64 = self.connection.query_row(
                sql,
                [&snapshot.snapshot_id],
                |row| row.get(0),
            )?;
            nonnegative_usize(value, "repository count")
        };
        let files_seen = count("SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=?")?;
        let text_files = count("SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='text'")?;
        let binary_files = count("SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='binary'")?;
        let oversized_files = count("SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='oversized'")?;
        let symlinks = count("SELECT COUNT(*) FROM snapshot_files WHERE snapshot_id=? AND file_kind='symlink'")?;
        let symbols = count("SELECT COUNT(*) FROM snapshot_files f JOIN content_symbols s ON s.content_sha256=f.content_sha256 AND s.parser_key=f.parser_key WHERE f.snapshot_id=?")?;
        let dependency_edges = count("SELECT COUNT(*) FROM snapshot_files f JOIN content_links l ON l.content_sha256=f.content_sha256 AND l.parser_key=f.parser_key WHERE f.snapshot_id=?")?;
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
    }'''
text = text[:start] + replacement + text[end:]

text = re.sub(
    r"fn artifact_symbol_count\(tx: &Transaction<'_>, hash: &str, key: &str\) -> Result<usize, RepoError> \{.*?\}\n",
    '''fn artifact_symbol_count(tx: &Transaction<'_>, hash: &str, key: &str) -> Result<usize, RepoError> {
    let value: i64 = tx.query_row(
        "SELECT COUNT(*) FROM content_symbols WHERE content_sha256=? AND parser_key=?",
        params![hash, key],
        |row| row.get(0),
    )?;
    nonnegative_usize(value, "artifact symbol count")
}
''',
    text,
    count=1,
)
text = re.sub(
    r"fn artifact_link_count\(tx: &Transaction<'_>, hash: &str, key: &str\) -> Result<usize, RepoError> \{.*?\}\n",
    '''fn artifact_link_count(tx: &Transaction<'_>, hash: &str, key: &str) -> Result<usize, RepoError> {
    let value: i64 = tx.query_row(
        "SELECT COUNT(*) FROM content_links WHERE content_sha256=? AND parser_key=?",
        params![hash, key],
        |row| row.get(0),
    )?;
    nonnegative_usize(value, "artifact link count")
}
''',
    text,
    count=1,
)

marker = "fn validate_query(query:&SearchQuery,policy:&IndexPolicy)->Result<(),RepoError>{"
helper = '''fn nonnegative_usize(value: i64, label: &str) -> Result<usize, RepoError> {
    usize::try_from(value).map_err(|_| RepoError::Integrity(format!("negative or overflowing {label}: {value}")))
}

'''
if marker not in text:
    raise SystemExit("count conversion helper anchor missing")
text = text.replace(marker, helper + marker, 1)
text = text.replace(
    "if query.text.len()>policy.max_query_bytes{return Err(RepoError::QueryTooLarge(query.text.len()));}",
    "if query.text.len()>policy.max_query_bytes{return Err(RepoError::QueryTooLarge(query.text.len()));}if query.min_score_micros<0{return Err(RepoError::Integrity(\"query score threshold cannot be negative\".to_owned()));}",
    1,
)
lib.write_text(text, encoding="utf-8")
