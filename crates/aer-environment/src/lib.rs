//! Baseline environment/dependency identity for repeatable evidence.

use std::{
    env,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use aer_exec::{CommandSpec, ExecutionPolicy, LocalProcessExecutor, SideEffectClass};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIdentity {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LockfileIdentity {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentSignal {
    pub name: String,
    pub value_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentFingerprint {
    pub os: String,
    pub architecture: String,
    pub family: String,
    pub os_version: Option<String>,
    pub shell: Option<String>,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub tools: Vec<ToolIdentity>,
    pub lockfiles: Vec<LockfileIdentity>,
    pub environment_signals: Vec<EnvironmentSignal>,
    pub digest: String,
}

impl EnvironmentFingerprint {
    pub fn discover(repo_root: impl AsRef<Path>) -> Result<Self, EnvironmentError> {
        let repo_root = repo_root
            .as_ref()
            .canonicalize()
            .map_err(EnvironmentError::Io)?;
        let policy =
            ExecutionPolicy::trusted_workspace(&repo_root, Duration::from_secs(5), 16 * 1024)?;

        let mut tools = ["git", "rustc", "cargo"]
            .into_iter()
            .map(|name| ToolIdentity {
                name: name.to_owned(),
                version: probe_tool(&policy, &repo_root, name),
            })
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));

        let mut lockfiles = known_lockfiles(&repo_root)?;
        lockfiles.sort_by(|left, right| left.path.cmp(&right.path));

        let mut environment_signals = selected_environment_signals();
        environment_signals.sort_by(|left, right| left.name.cmp(&right.name));

        let os_version = probe_os_version(&policy, &repo_root);
        let shell = env::var_os(shell_variable_name())
            .and_then(|value| PathBuf::from(value).file_name().map(|name| name.to_owned()))
            .map(|name| name.to_string_lossy().into_owned());
        let locale = env::var("LC_ALL")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| env::var("LANG").ok().filter(|value| !value.is_empty()));
        let timezone = env::var("TZ").ok().filter(|value| !value.is_empty());

        let mut fingerprint = Self {
            os: env::consts::OS.to_owned(),
            architecture: env::consts::ARCH.to_owned(),
            family: env::consts::FAMILY.to_owned(),
            os_version,
            shell,
            locale,
            timezone,
            tools,
            lockfiles,
            environment_signals,
            digest: String::new(),
        };
        fingerprint.digest = fingerprint.compute_digest();
        Ok(fingerprint)
    }

    fn compute_digest(&self) -> String {
        let mut hasher = Sha256::new();
        update_field(&mut hasher, "os", &self.os);
        update_field(&mut hasher, "architecture", &self.architecture);
        update_field(&mut hasher, "family", &self.family);
        update_field(
            &mut hasher,
            "os_version",
            self.os_version.as_deref().unwrap_or("<unknown>"),
        );
        update_field(
            &mut hasher,
            "shell",
            self.shell.as_deref().unwrap_or("<unknown>"),
        );
        update_field(
            &mut hasher,
            "locale",
            self.locale.as_deref().unwrap_or("<unknown>"),
        );
        update_field(
            &mut hasher,
            "timezone",
            self.timezone.as_deref().unwrap_or("<unknown>"),
        );
        for tool in &self.tools {
            update_field(&mut hasher, "tool.name", &tool.name);
            update_field(
                &mut hasher,
                "tool.version",
                tool.version.as_deref().unwrap_or("<unavailable>"),
            );
        }
        for lockfile in &self.lockfiles {
            update_field(&mut hasher, "lock.path", &lockfile.path.to_string_lossy());
            update_field(&mut hasher, "lock.sha256", &lockfile.sha256);
        }
        for signal in &self.environment_signals {
            update_field(&mut hasher, "env.name", &signal.name);
            update_field(&mut hasher, "env.sha256", &signal.value_sha256);
        }
        format!("{:x}", hasher.finalize())
    }
}

fn probe_tool(policy: &ExecutionPolicy, root: &Path, name: &str) -> Option<String> {
    let spec = CommandSpec::new(name, root, SideEffectClass::PureRead).args(["--version"]);
    let result = LocalProcessExecutor.execute(policy, spec).ok()?;
    if !result.success || result.stdout.truncated {
        return None;
    }
    first_nonempty_line(&result.stdout.preview)
}

#[cfg(windows)]
fn probe_os_version(policy: &ExecutionPolicy, root: &Path) -> Option<String> {
    let spec = CommandSpec::new("cmd", root, SideEffectClass::PureRead).args(["/C", "ver"]);
    let result = LocalProcessExecutor.execute(policy, spec).ok()?;
    if !result.success || result.stdout.truncated {
        return None;
    }
    first_nonempty_line(&result.stdout.preview)
}

#[cfg(not(windows))]
fn probe_os_version(policy: &ExecutionPolicy, root: &Path) -> Option<String> {
    let spec = CommandSpec::new("uname", root, SideEffectClass::PureRead).args(["-sr"]);
    let result = LocalProcessExecutor.execute(policy, spec).ok()?;
    if !result.success || result.stdout.truncated {
        return None;
    }
    first_nonempty_line(&result.stdout.preview)
}

fn first_nonempty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn known_lockfiles(root: &Path) -> Result<Vec<LockfileIdentity>, EnvironmentError> {
    const NAMES: &[&str] = &[
        "Cargo.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "uv.lock",
        "poetry.lock",
        "Pipfile.lock",
        "go.sum",
    ];
    let mut identities = Vec::new();
    for name in NAMES {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path).map_err(EnvironmentError::Io)?;
        identities.push(LockfileIdentity {
            path: PathBuf::from(name),
            sha256: sha256(&bytes),
        });
    }
    Ok(identities)
}

fn selected_environment_signals() -> Vec<EnvironmentSignal> {
    const NAMES: &[&str] = &["PATH", "RUSTFLAGS", "CARGO_BUILD_TARGET", "CI", "NO_COLOR"];
    NAMES
        .iter()
        .filter_map(|name| {
            env::var_os(name).map(|value| EnvironmentSignal {
                name: (*name).to_owned(),
                value_sha256: sha256(value.to_string_lossy().as_bytes()),
            })
        })
        .collect()
}

#[cfg(windows)]
const fn shell_variable_name() -> &'static str {
    "COMSPEC"
}

#[cfg(not(windows))]
const fn shell_variable_name() -> &'static str {
    "SHELL"
}

fn update_field(hasher: &mut Sha256, name: &str, value: &str) {
    hasher.update(name.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hasher.update([0xff]);
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug)]
pub enum EnvironmentError {
    Io(std::io::Error),
    Execution(aer_exec::ExecutionError),
}

impl fmt::Display for EnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Execution(error) => error.fmt(formatter),
        }
    }
}

impl Error for EnvironmentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Execution(error) => Some(error),
        }
    }
}

impl From<aer_exec::ExecutionError> for EnvironmentError {
    fn from(value: aer_exec::ExecutionError) -> Self {
        Self::Execution(value)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::EnvironmentFingerprint;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "everything-env-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    #[test]
    fn lockfile_content_changes_environment_digest() {
        let root = temp_dir("lock");
        fs::write(root.join("Cargo.lock"), "version = 1\n").expect("lock v1");
        let first = EnvironmentFingerprint::discover(&root).expect("first fingerprint");
        fs::write(root.join("Cargo.lock"), "version = 2\n").expect("lock v2");
        let second = EnvironmentFingerprint::discover(&root).expect("second fingerprint");
        assert_ne!(first.digest, second.digest);
        assert_ne!(first.lockfiles, second.lockfiles);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn only_explicit_nonsecret_environment_names_are_fingerprinted() {
        let root = temp_dir("signals");
        let fingerprint = EnvironmentFingerprint::discover(&root).expect("fingerprint");
        assert!(
            fingerprint
                .environment_signals
                .iter()
                .all(|signal| matches!(
                    signal.name.as_str(),
                    "PATH" | "RUSTFLAGS" | "CARGO_BUILD_TARGET" | "CI" | "NO_COLOR"
                ))
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
