//! Shared measurement plumbing for the non-production provider harnesses.
//!
//! The cache lab, the authority-split acceptance matrix and the Claude Code
//! parity benchmark all need the same three things: a bounded subprocess runner
//! that cannot inherit the operator's shell, a filtered Git-backed shadow of the
//! repository so retrieval cannot reach harness material, and deterministic
//! executable discovery. They are factored here so a fix to any of them applies
//! to every measurement rather than to one copy of it.
//!
//! Nothing here belongs to the product. It measures the product.

pub mod process;
pub mod shadow;
pub mod stats;

use std::{error::Error, fmt, io, path::PathBuf};

/// Failures shared by every harness.
#[derive(Debug)]
pub enum HarnessError {
    Executable(String),
    Version(Option<i32>),
    ShadowEscape(PathBuf),
    ShadowLimit {
        files: usize,
        bytes: u64,
    },
    ShadowGit {
        command: String,
        exit_code: Option<i32>,
        detail: String,
    },
    TimedOut {
        seconds: u64,
    },
    MissingPipe(&'static str),
    Worker(&'static str),
    Io(io::Error),
    Clock,
}

impl fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Executable(name) => write!(formatter, "{name} executable not found on PATH"),
            Self::Version(code) => write!(formatter, "executable --version failed with {code:?}"),
            Self::ShadowEscape(path) => {
                write!(
                    formatter,
                    "shadow workspace path escaped root: {}",
                    path.display()
                )
            }
            Self::ShadowLimit { files, bytes } => write!(
                formatter,
                "shadow workspace exceeded safety limit: {files} files, {bytes} bytes"
            ),
            Self::ShadowGit {
                command,
                exit_code,
                detail,
            } => write!(
                formatter,
                "shadow git command `{command}` failed with {exit_code:?}: {detail}"
            ),
            Self::TimedOut { seconds } => {
                write!(formatter, "child process timed out after {seconds} seconds")
            }
            Self::MissingPipe(pipe) => write!(formatter, "missing child {pipe} pipe"),
            Self::Worker(worker) => write!(formatter, "{worker} worker panicked"),
            Self::Io(error) => error.fmt(formatter),
            Self::Clock => formatter.write_str("system clock is before UNIX_EPOCH"),
        }
    }
}

impl Error for HarnessError {}

impl From<io::Error> for HarnessError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// First lines of a captured stream, bounded so an error message cannot become a log dump.
#[must_use]
pub fn preview(value: &str) -> String {
    value
        .lines()
        .take(12)
        .collect::<Vec<_>>()
        .join(" | ")
        .chars()
        .take(1200)
        .collect()
}

/// Leading characters of a digest, for human-readable tables.
#[must_use]
pub fn short(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

/// Lowercase hex SHA-256 of arbitrary bytes.
#[must_use]
pub fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};

    let digest = Sha256::digest(bytes);
    let mut rendered = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use fmt::Write as _;
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_and_hex() {
        let digest = hex_sha256(b"aer");
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(digest, hex_sha256(b"aer"));
        assert_ne!(digest, hex_sha256(b"aer "));
    }

    #[test]
    fn preview_bounds_both_lines_and_characters() {
        let long = (0..100)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let preview = preview(&long);
        assert_eq!(preview.matches(" | ").count(), 11);
        assert!(preview.chars().count() <= 1200);
    }

    #[test]
    fn short_never_panics_on_short_input() {
        assert_eq!(short("abc"), "abc");
        assert_eq!(short("0123456789abcdef"), "0123456789ab");
    }
}
