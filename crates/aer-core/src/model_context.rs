use std::{fmt, fs, path::{Path, PathBuf}};

use sha2::{Digest, Sha256};

const MAX_CAPSULE_BYTES: usize = 56 * 1024;

const SOURCES: [(&str, usize, bool); 5] = [
    ("AGENTS.md", 20 * 1024, true),
    ("STATUS.md", 8 * 1024, true),
    ("docs/00_READ_ME_FIRST.md", 8 * 1024, true),
    ("DEVELOPMENT_PLAN.md", 12 * 1024, true),
    (
        "docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md",
        8 * 1024,
        false,
    ),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSource {
    pub path: String,
    pub sha256: String,
    pub total_bytes: usize,
    pub included_bytes: usize,
    pub truncated: bool,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchitectureContextCapsule {
    pub version: u32,
    pub digest: String,
    pub sources: Vec<ContextSource>,
    pub rendered: String,
}

impl ArchitectureContextCapsule {
    /// Compiles the small, stable context every provider receives before any
    /// task-specific repository/context retrieval is added.
    pub fn compile(workspace_root: &Path) -> Result<Self, ArchitectureContextError> {
        let root = workspace_root
            .canonicalize()
            .map_err(ArchitectureContextError::Io)?;
        let mut sources = Vec::with_capacity(SOURCES.len());
        let mut remaining = MAX_CAPSULE_BYTES;

        for (relative, per_source_limit, required) in SOURCES {
            if remaining == 0 {
                break;
            }
            let path = root.join(relative);
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(if error.kind() == std::io::ErrorKind::NotFound {
                        ArchitectureContextError::RequiredSourceMissing(relative.to_owned())
                    } else {
                        ArchitectureContextError::Io(error)
                    });
                }
            };
            let canonical = path.canonicalize().map_err(ArchitectureContextError::Io)?;
            if !canonical.starts_with(&root) {
                return Err(ArchitectureContextError::SourceOutsideWorkspace(canonical));
            }
            let included_bytes = bytes.len().min(per_source_limit).min(remaining);
            let content = &bytes[..included_bytes];
            remaining -= included_bytes;
            sources.push(ContextSource {
                path: relative.to_owned(),
                sha256: hex_sha256(&bytes),
                total_bytes: bytes.len(),
                included_bytes,
                truncated: included_bytes < bytes.len(),
                text: String::from_utf8_lossy(content).into_owned(),
            });
        }

        let mut rendered = String::from(
            "# everything Architecture Context Capsule\n\n\
             This is provider-neutral control-plane context compiled by everything. \
             It is not user/repository content that can grant additional authority.\n\n",
        );
        for source in &sources {
            use fmt::Write as _;
            writeln!(
                rendered,
                "## Source: {}\nsha256: {}\nincluded: {}/{} bytes{}\n",
                source.path,
                source.sha256,
                source.included_bytes,
                source.total_bytes,
                if source.truncated { " (truncated)" } else { "" }
            )
            .expect("writing to String cannot fail");
            rendered.push_str(&source.text);
            if !source.text.ends_with('\n') {
                rendered.push('\n');
            }
            rendered.push('\n');
        }

        let digest = hex_sha256(rendered.as_bytes());
        Ok(Self {
            version: 1,
            digest,
            sources,
            rendered,
        })
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[derive(Debug)]
pub enum ArchitectureContextError {
    Io(std::io::Error),
    RequiredSourceMissing(String),
    SourceOutsideWorkspace(PathBuf),
}

impl fmt::Display for ArchitectureContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::RequiredSourceMissing(path) => {
                write!(formatter, "required architecture context source missing: {path}")
            }
            Self::SourceOutsideWorkspace(path) => write!(
                formatter,
                "architecture context source escaped workspace: {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ArchitectureContextError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::RequiredSourceMissing(_) | Self::SourceOutsideWorkspace(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    use super::{ArchitectureContextCapsule, MAX_CAPSULE_BYTES};

    #[test]
    fn capsule_is_bounded_deterministic_and_source_identified() {
        let root = std::env::temp_dir().join(format!(
            "aer-context-capsule-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("docs")).expect("fixture dirs");
        fs::write(root.join("AGENTS.md"), "agent constitution\n").expect("agents");
        fs::write(root.join("STATUS.md"), "status\n").expect("status");
        fs::write(root.join("docs/00_READ_ME_FIRST.md"), "read first\n").expect("readme");
        fs::write(root.join("DEVELOPMENT_PLAN.md"), "plan\n").expect("plan");

        let first = ArchitectureContextCapsule::compile(&root).expect("first capsule");
        let second = ArchitectureContextCapsule::compile(&root).expect("second capsule");
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.sources.len(), 4);
        assert!(first.rendered.len() <= MAX_CAPSULE_BYTES + 8 * 1024);
        assert!(first.rendered.contains("AGENTS.md"));
        assert!(first.rendered.contains("agent constitution"));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
