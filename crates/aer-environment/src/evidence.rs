//! Command evidence bound to repository and environment identity.

use std::{error::Error, fmt, path::PathBuf};

use aer_exec::{ProcessResult, SecurityProfile, lowercase_hex};
use sha2::{Digest, Sha256};

use crate::EnvironmentFingerprint;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandExecutionEvidence {
    pub repo_id: String,
    pub environment_digest: String,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub timed_out: bool,
    pub duration_ms: u128,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub security_profile: SecurityProfile,
    pub evidence_digest: String,
}

impl CommandExecutionEvidence {
    pub fn bind(
        repo_id: impl Into<String>,
        environment: &EnvironmentFingerprint,
        result: &ProcessResult,
    ) -> Result<Self, EvidenceError> {
        let repo_id = repo_id.into();
        if repo_id.trim().is_empty() {
            return Err(EvidenceError::EmptyRepoIdentity);
        }
        if environment.digest.trim().is_empty() {
            return Err(EvidenceError::EmptyEnvironmentIdentity);
        }

        let mut evidence = Self {
            repo_id,
            environment_digest: environment.digest.clone(),
            argv: result.argv.clone(),
            cwd: result.cwd.clone(),
            exit_code: result.exit_code,
            success: result.success,
            timed_out: result.timed_out,
            duration_ms: result.duration_ms,
            stdout_sha256: result.stdout.sha256.clone(),
            stderr_sha256: result.stderr.sha256.clone(),
            stdout_bytes: result.stdout.total_bytes,
            stderr_bytes: result.stderr.total_bytes,
            security_profile: result.security_profile,
            evidence_digest: String::new(),
        };
        evidence.evidence_digest = evidence.compute_digest();
        Ok(evidence)
    }

    fn compute_digest(&self) -> String {
        let mut hasher = Sha256::new();
        update(&mut hasher, "repo_id", &self.repo_id);
        update(
            &mut hasher,
            "environment_digest",
            &self.environment_digest,
        );
        for argument in &self.argv {
            update(&mut hasher, "argv", argument);
        }
        update(&mut hasher, "cwd", &self.cwd.to_string_lossy());
        update(
            &mut hasher,
            "exit_code",
            &self
                .exit_code
                .map_or_else(|| "signal-or-unknown".to_owned(), |code| code.to_string()),
        );
        update(&mut hasher, "success", if self.success { "1" } else { "0" });
        update(
            &mut hasher,
            "timed_out",
            if self.timed_out { "1" } else { "0" },
        );
        update(&mut hasher, "duration_ms", &self.duration_ms.to_string());
        update(&mut hasher, "stdout_sha256", &self.stdout_sha256);
        update(&mut hasher, "stderr_sha256", &self.stderr_sha256);
        update(&mut hasher, "stdout_bytes", &self.stdout_bytes.to_string());
        update(&mut hasher, "stderr_bytes", &self.stderr_bytes.to_string());
        update(
            &mut hasher,
            "security_profile",
            match self.security_profile {
                SecurityProfile::DirectHostProcess => "direct_host_process",
            },
        );
        lowercase_hex(hasher.finalize().as_ref())
    }
}

fn update(hasher: &mut Sha256, name: &str, value: &str) {
    hasher.update(name.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hasher.update([0xff]);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvidenceError {
    EmptyRepoIdentity,
    EmptyEnvironmentIdentity,
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRepoIdentity => formatter.write_str("command evidence requires repo identity"),
            Self::EmptyEnvironmentIdentity => {
                formatter.write_str("command evidence requires environment identity")
            }
        }
    }
}

impl Error for EvidenceError {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aer_exec::{CapturedStream, ProcessResult, SecurityProfile};

    use super::CommandExecutionEvidence;
    use crate::EnvironmentFingerprint;

    fn environment(digest: &str) -> EnvironmentFingerprint {
        EnvironmentFingerprint {
            os: "windows".to_owned(),
            architecture: "x86_64".to_owned(),
            family: "windows".to_owned(),
            os_version: None,
            shell: None,
            locale: None,
            timezone: None,
            tools: Vec::new(),
            lockfiles: Vec::new(),
            environment_signals: Vec::new(),
            digest: digest.to_owned(),
        }
    }

    fn result() -> ProcessResult {
        ProcessResult {
            argv: vec!["git".to_owned(), "status".to_owned()],
            cwd: PathBuf::from("repo"),
            exit_code: Some(0),
            success: true,
            timed_out: false,
            duration_ms: 12,
            stdout: CapturedStream {
                preview: b"clean".to_vec(),
                sha256: "stdout".to_owned(),
                total_bytes: 5,
                truncated: false,
            },
            stderr: CapturedStream {
                preview: Vec::new(),
                sha256: "stderr".to_owned(),
                total_bytes: 0,
                truncated: false,
            },
            security_profile: SecurityProfile::DirectHostProcess,
        }
    }

    #[test]
    fn evidence_identity_changes_with_repo_or_environment() {
        let process = result();
        let first = CommandExecutionEvidence::bind("repo-a", &environment("env-a"), &process)
            .expect("first evidence");
        let repo_changed = CommandExecutionEvidence::bind("repo-b", &environment("env-a"), &process)
            .expect("repo evidence");
        let env_changed = CommandExecutionEvidence::bind("repo-a", &environment("env-b"), &process)
            .expect("env evidence");

        assert_ne!(first.evidence_digest, repo_changed.evidence_digest);
        assert_ne!(first.evidence_digest, env_changed.evidence_digest);
    }

    #[test]
    fn evidence_binds_process_hashes_without_raw_environment_values() {
        let process = result();
        let evidence = CommandExecutionEvidence::bind("repo-a", &environment("env-a"), &process)
            .expect("evidence");
        assert_eq!(evidence.stdout_sha256, "stdout");
        assert_eq!(evidence.stderr_sha256, "stderr");
        assert_eq!(evidence.environment_digest, "env-a");
        assert!(!evidence.evidence_digest.is_empty());
    }
}
