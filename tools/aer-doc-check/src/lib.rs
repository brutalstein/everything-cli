use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

pub const MANIFEST_RELATIVE_PATH: &str = "docs/MANIFEST.sha256";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityReport {
    pub numbered_docs: usize,
    pub accepted_adrs: usize,
    pub core_schemas: usize,
    pub examples: usize,
    pub manifest_entries: usize,
}

impl IntegrityReport {
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "documentation integrity: PASS\nnumbered docs: {}\naccepted ADRs: {}\ncore schemas: {}\nexamples: {}\nmanifest entries: {}",
            self.numbered_docs,
            self.accepted_adrs,
            self.core_schemas,
            self.examples,
            self.manifest_entries
        )
    }
}

#[derive(Debug)]
pub enum IntegrityError {
    Io { path: PathBuf, source: io::Error },
    MissingManifest(PathBuf),
    MalformedManifestLine { line: usize, value: String },
    DuplicateManifestEntry(String),
    InvalidHash { line: usize, value: String },
    MissingManifestEntry(String),
    ExtraManifestEntry(String),
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::MissingManifest(path) => {
                write!(formatter, "documentation manifest is missing: {}", path.display())
            }
            Self::MalformedManifestLine { line, value } => {
                write!(formatter, "malformed manifest line {line}: {value:?}")
            }
            Self::DuplicateManifestEntry(path) => {
                write!(formatter, "duplicate manifest entry: {path}")
            }
            Self::InvalidHash { line, value } => {
                write!(formatter, "invalid SHA-256 on manifest line {line}: {value:?}")
            }
            Self::MissingManifestEntry(path) => write!(formatter, "manifest missing entry: {path}"),
            Self::ExtraManifestEntry(path) => write!(formatter, "manifest has extra entry: {path}"),
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "documentation hash mismatch for {path}: expected {expected}, actual {actual}"
            ),
        }
    }
}

impl Error for IntegrityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[must_use]
pub fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("aer-doc-check must live under tools/<crate>")
        .to_path_buf()
}

pub fn check_repository(root: &Path) -> Result<IntegrityReport, IntegrityError> {
    let manifest_path = root.join(MANIFEST_RELATIVE_PATH);
    if !manifest_path.is_file() {
        return Err(IntegrityError::MissingManifest(manifest_path));
    }

    let manifest = parse_manifest(&manifest_path)?;
    let mut actual_paths = BTreeSet::new();
    collect_docs_files(root, &root.join("docs"), &mut actual_paths)?;
    actual_paths.remove(MANIFEST_RELATIVE_PATH);

    let manifest_paths = manifest.keys().cloned().collect::<BTreeSet<_>>();
    for missing in actual_paths.difference(&manifest_paths) {
        return Err(IntegrityError::MissingManifestEntry(missing.clone()));
    }
    for extra in manifest_paths.difference(&actual_paths) {
        return Err(IntegrityError::ExtraManifestEntry(extra.clone()));
    }

    for (relative_path, expected_hash) in &manifest {
        let absolute_path = root.join(relative_path);
        let actual_hash = sha256_file(&absolute_path)?;
        if &actual_hash != expected_hash {
            return Err(IntegrityError::HashMismatch {
                path: relative_path.clone(),
                expected: expected_hash.clone(),
                actual: actual_hash,
            });
        }
    }

    let numbered_docs = actual_paths
        .iter()
        .filter(|path| is_numbered_architecture_doc(path))
        .count();
    let accepted_adrs = actual_paths
        .iter()
        .filter(|path| path.starts_with("docs/adrs/ADR-") && path.ends_with(".md"))
        .count();
    let core_schemas = actual_paths
        .iter()
        .filter(|path| path.starts_with("docs/schemas/") && path.ends_with(".schema.json"))
        .count();
    let examples = actual_paths
        .iter()
        .filter(|path| path.starts_with("docs/examples/"))
        .count();

    Ok(IntegrityReport {
        numbered_docs,
        accepted_adrs,
        core_schemas,
        examples,
        manifest_entries: manifest.len(),
    })
}

pub fn write_manifest(root: &Path) -> Result<IntegrityReport, IntegrityError> {
    let mut actual_paths = BTreeSet::new();
    collect_docs_files(root, &root.join("docs"), &mut actual_paths)?;
    actual_paths.remove(MANIFEST_RELATIVE_PATH);

    let manifest_path = root.join(MANIFEST_RELATIVE_PATH);
    let mut manifest_file = fs::File::create(&manifest_path).map_err(|source| IntegrityError::Io {
        path: manifest_path.clone(),
        source,
    })?;

    for relative_path in &actual_paths {
        let hash = sha256_file(&root.join(relative_path))?;
        writeln!(manifest_file, "{hash}  {relative_path}").map_err(|source| IntegrityError::Io {
            path: manifest_path.clone(),
            source,
        })?;
    }

    check_repository(root)
}

fn is_numbered_architecture_doc(path: &str) -> bool {
    let Some(file_name) = path.strip_prefix("docs/") else {
        return false;
    };
    if file_name.contains('/') || !file_name.ends_with(".md") {
        return false;
    }
    let bytes = file_name.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit() && bytes[2] == b'_'
}

fn parse_manifest(path: &Path) -> Result<BTreeMap<String, String>, IntegrityError> {
    let contents = fs::read_to_string(path).map_err(|source| IntegrityError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut entries = BTreeMap::new();
    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((hash, relative_path)) = line.split_once("  ") else {
            return Err(IntegrityError::MalformedManifestLine {
                line: line_number,
                value: raw_line.to_owned(),
            });
        };
        let relative_path = relative_path.trim();
        if relative_path.is_empty() {
            return Err(IntegrityError::MalformedManifestLine {
                line: line_number,
                value: raw_line.to_owned(),
            });
        }
        if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(IntegrityError::InvalidHash {
                line: line_number,
                value: hash.to_owned(),
            });
        }
        if entries
            .insert(relative_path.to_owned(), hash.to_ascii_lowercase())
            .is_some()
        {
            return Err(IntegrityError::DuplicateManifestEntry(
                relative_path.to_owned(),
            ));
        }
    }
    Ok(entries)
}

fn sha256_file(path: &Path) -> Result<String, IntegrityError> {
    let bytes = fs::read(path).map_err(|source| IntegrityError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

fn collect_docs_files(
    root: &Path,
    directory: &Path,
    paths: &mut BTreeSet<String>,
) -> Result<(), IntegrityError> {
    let entries = fs::read_dir(directory).map_err(|source| IntegrityError::Io {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| IntegrityError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| IntegrityError::Io {
            path: entry.path(),
            source,
        })?;
        if file_type.is_dir() {
            collect_docs_files(root, &entry.path(), paths)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .expect("walked path must remain under repository root")
                .to_string_lossy()
                .replace('\\', "/");
            paths.insert(relative);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_repository, repository_root};

    #[test]
    fn checked_in_docs_inventory_is_consistent() {
        let report = check_repository(&repository_root())
            .expect("checked-in architecture documentation should be internally consistent");

        assert_eq!(report.numbered_docs, 45);
        assert_eq!(report.accepted_adrs, 9);
        assert_eq!(report.core_schemas, 15);
        assert_eq!(report.examples, 3);
        assert_eq!(report.manifest_entries, 75);
    }
}
