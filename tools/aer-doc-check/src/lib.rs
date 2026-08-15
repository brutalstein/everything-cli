//! Cross-platform, dependency-free checks for the architecture documentation
//! inventory and manifest coverage.

use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use aer_domain::contracts::CORE_CONTRACTS;

const EXAMPLES: [&str; 3] = [
    "docs/examples/example-handoff.json",
    "docs/examples/example-project-ir.yaml",
    "docs/examples/example-proof-manifest.yaml",
];

#[derive(Debug)]
pub struct IntegrityReport {
    pub numbered_docs: usize,
    pub accepted_adrs: usize,
    pub core_schemas: usize,
    pub examples: usize,
    pub manifest_entries: usize,
}

#[derive(Debug)]
pub enum IntegrityError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    ExpectedExactlyOne {
        label: String,
        matches: usize,
    },
    MissingPath(PathBuf),
    MalformedManifestLine {
        line: usize,
        text: String,
    },
    DuplicateManifestPath(String),
    ManifestCoverageMismatch {
        missing: Vec<String>,
        stale: Vec<String>,
    },
}

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::ExpectedExactlyOne { label, matches } => {
                write!(formatter, "expected exactly one {label}, found {matches}")
            }
            Self::MissingPath(path) => {
                write!(formatter, "required path is missing: {}", path.display())
            }
            Self::MalformedManifestLine { line, text } => {
                write!(
                    formatter,
                    "malformed docs/MANIFEST.sha256 line {line}: {text}"
                )
            }
            Self::DuplicateManifestPath(path) => {
                write!(formatter, "duplicate manifest path: {path}")
            }
            Self::ManifestCoverageMismatch { missing, stale } => write!(
                formatter,
                "manifest coverage mismatch; missing entries: {missing:?}; stale entries: {stale:?}"
            ),
        }
    }
}

impl std::error::Error for IntegrityError {}

#[must_use]
pub fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn check_repository(root: &Path) -> Result<IntegrityReport, IntegrityError> {
    let docs_dir = root.join("docs");
    let adr_dir = docs_dir.join("adrs");

    let numbered_docs =
        check_numbered_files(&docs_dir, 0, 44, 2, "", "_", ".md", "architecture doc")?;
    let accepted_adrs =
        check_numbered_files(&adr_dir, 1, 9, 4, "ADR-", "-", ".md", "accepted ADR")?;

    for contract in CORE_CONTRACTS {
        require_path(root, contract.descriptor().schema_path)?;
    }
    for example in EXAMPLES {
        require_path(root, example)?;
    }
    require_path(root, "docs/MANIFEST.sha256")?;

    let manifest_entries = check_manifest_coverage(root)?;

    Ok(IntegrityReport {
        numbered_docs,
        accepted_adrs,
        core_schemas: CORE_CONTRACTS.len(),
        examples: EXAMPLES.len(),
        manifest_entries,
    })
}

fn check_numbered_files(
    directory: &Path,
    start: usize,
    end: usize,
    width: usize,
    leading: &str,
    separator: &str,
    suffix: &str,
    label: &str,
) -> Result<usize, IntegrityError> {
    let names = read_file_names(directory)?;

    for number in start..=end {
        let prefix = format!("{leading}{number:0width$}{separator}");
        let matches = names
            .iter()
            .filter(|name| name.starts_with(&prefix) && name.ends_with(suffix))
            .count();
        if matches != 1 {
            return Err(IntegrityError::ExpectedExactlyOne {
                label: format!("{label} with prefix {prefix}"),
                matches,
            });
        }
    }

    Ok(end - start + 1)
}

fn read_file_names(directory: &Path) -> Result<Vec<String>, IntegrityError> {
    let entries = fs::read_dir(directory).map_err(|source| IntegrityError::Io {
        path: directory.to_path_buf(),
        source,
    })?;

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| IntegrityError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        if entry
            .file_type()
            .map_err(|source| IntegrityError::Io {
                path: entry.path(),
                source,
            })?
            .is_file()
        {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

fn require_path(root: &Path, relative: &str) -> Result<(), IntegrityError> {
    let path = root.join(relative);
    if path.is_file() {
        Ok(())
    } else {
        Err(IntegrityError::MissingPath(path))
    }
}

fn check_manifest_coverage(root: &Path) -> Result<usize, IntegrityError> {
    let manifest_path = root.join("docs/MANIFEST.sha256");
    let manifest = fs::read_to_string(&manifest_path).map_err(|source| IntegrityError::Io {
        path: manifest_path,
        source,
    })?;

    let mut manifest_paths = BTreeSet::new();
    for (index, line) in manifest.lines().enumerate() {
        let mut fields = line.split_whitespace();
        let Some(hash) = fields.next() else {
            continue;
        };
        let Some(path) = fields.next() else {
            return Err(IntegrityError::MalformedManifestLine {
                line: index + 1,
                text: line.to_owned(),
            });
        };
        if fields.next().is_some()
            || hash.len() != 64
            || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !path.starts_with("docs/")
        {
            return Err(IntegrityError::MalformedManifestLine {
                line: index + 1,
                text: line.to_owned(),
            });
        }
        if !manifest_paths.insert(path.to_owned()) {
            return Err(IntegrityError::DuplicateManifestPath(path.to_owned()));
        }
    }

    let mut actual_paths = BTreeSet::new();
    collect_docs_files(root, &root.join("docs"), &mut actual_paths)?;
    actual_paths.remove("docs/MANIFEST.sha256");

    let missing: Vec<_> = actual_paths.difference(&manifest_paths).cloned().collect();
    let stale: Vec<_> = manifest_paths.difference(&actual_paths).cloned().collect();
    if !missing.is_empty() || !stale.is_empty() {
        return Err(IntegrityError::ManifestCoverageMismatch { missing, stale });
    }

    Ok(manifest_paths.len())
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
        assert_eq!(report.manifest_entries, 72);
    }
}
