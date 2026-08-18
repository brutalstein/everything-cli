//! Typed local command boundary for trusted host-side tooling.
//!
//! This crate intentionally does **not** claim to be a strong sandbox. It provides
//! deterministic argv execution, workspace/cwd checks, bounded output capture,
//! timeout/kill behavior, environment minimization, and explicit side-effect
//! classification. Untrusted agent-generated execution must later be wrapped by
//! a real sandbox backend rather than treating this adapter as equivalent.

use std::{
    env,
    error::Error,
    ffi::{OsStr, OsString},
    fmt,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SideEffectClass {
    PureRead,
    WorkspaceWrite,
    ProcessExecution,
    NetworkRead,
    NetworkWrite,
    ExternalMutation,
    CredentialUse,
    Privileged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecurityProfile {
    /// Direct child process on the host. Filesystem/network isolation is not
    /// enforced by this adapter and must never be represented as sandboxed.
    DirectHostProcess,
}

/// The policy-driven execution profiles of `docs/13`.
///
/// A trust level says what a run is *authorized* to do. It never says what the
/// substrate can actually enforce: that is [`IsolationReport`]. The two are
/// deliberately separate types so that a permissive profile can never imply a
/// sandbox.
///
/// `docs/13` also defines `networked-build` and `privileged-special`. They are
/// absent here on purpose: no policy path selects them yet, and a level that
/// nothing can select would be an authority claim with no implementation
/// behind it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustLevel {
    /// Repository reads for scouting and review. No writes.
    ReadOnly,
    /// Reads and writes confined to an isolated worktree and task temp space.
    WorkspaceWrite,
}

impl TrustLevel {
    /// The stable identifier recorded in evidence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
        }
    }

    /// The side effects this profile may authorize.
    #[must_use]
    pub fn side_effects(self) -> Vec<SideEffectClass> {
        match self {
            Self::ReadOnly => vec![SideEffectClass::PureRead],
            Self::WorkspaceWrite => vec![
                SideEffectClass::PureRead,
                SideEffectClass::WorkspaceWrite,
                SideEffectClass::ProcessExecution,
            ],
        }
    }
}

/// The network classification of `docs/13`.
///
/// Widening this is a policy event. No model, prompt or tool response may
/// select a wider class than the run was created with. The intermediate classes
/// of `docs/13` appear once a substrate can actually distinguish them; naming
/// them before then would imply enforcement that does not exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkClass {
    /// No egress.
    None,
    /// Everything. High risk, and the honest label for any substrate that does
    /// not restrict egress at all.
    Unrestricted,
}

impl NetworkClass {
    /// The stable identifier recorded in evidence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Unrestricted => "unrestricted",
        }
    }
}

/// The isolation dimensions `docs/13` requires a sandbox to control.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum IsolationDimension {
    /// Writes confined to the worktree and task temp directories.
    Filesystem,
    /// Egress denied by default and classified when allowed.
    Network,
    /// Host secrets kept outside the reachable filesystem.
    Credentials,
    /// CPU, memory, PID, disk and wall-clock bounds.
    ProcessResources,
    /// No host Docker socket or equivalent high-authority interface.
    HostControlSockets,
    /// Held-out verification material immutable to whatever generated the change.
    VerifierAssets,
}

/// Every dimension a substrate must control before it may be called a sandbox.
pub const ISOLATION_DIMENSIONS: [IsolationDimension; 6] = [
    IsolationDimension::Filesystem,
    IsolationDimension::Network,
    IsolationDimension::Credentials,
    IsolationDimension::ProcessResources,
    IsolationDimension::HostControlSockets,
    IsolationDimension::VerifierAssets,
];

impl IsolationDimension {
    /// The stable identifier recorded in evidence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::Network => "network",
            Self::Credentials => "credentials",
            Self::ProcessResources => "process-resources",
            Self::HostControlSockets => "host-control-sockets",
            Self::VerifierAssets => "verifier-assets",
        }
    }
}

/// What a given execution substrate actually enforces.
///
/// This is reported, never assumed. `docs/13` requires that when strong
/// isolation is unavailable, the degraded profile is stated rather than treated
/// as equivalent. A report therefore only ever grows by a backend proving a
/// dimension, never by a caller asserting one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IsolationReport {
    backend: &'static str,
    enforced: Vec<IsolationDimension>,
}

impl IsolationReport {
    /// The honest report for a direct child process on the host.
    ///
    /// It enforces none of the six. Confining the working directory and
    /// clearing inherited environment variables is useful hygiene, but the child
    /// can still write anywhere the user can, open any socket and read any
    /// credential file, so claiming a dimension here would be false.
    #[must_use]
    pub fn host_process() -> Self {
        Self {
            backend: "direct-host-process",
            enforced: Vec::new(),
        }
    }

    /// The substrate this report describes.
    #[must_use]
    pub const fn backend(&self) -> &'static str {
        self.backend
    }

    /// Whether one dimension is actually enforced.
    #[must_use]
    pub fn enforces(&self, dimension: IsolationDimension) -> bool {
        self.enforced.contains(&dimension)
    }

    /// The dimensions this substrate does not control.
    #[must_use]
    pub fn unenforced(&self) -> Vec<IsolationDimension> {
        ISOLATION_DIMENSIONS
            .into_iter()
            .filter(|dimension| !self.enforces(*dimension))
            .collect()
    }

    /// Whether this substrate satisfies every required dimension.
    #[must_use]
    pub fn is_strong(&self) -> bool {
        self.unenforced().is_empty()
    }
}

impl fmt::Display for IsolationReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_strong() {
            return write!(
                formatter,
                "{} enforces every isolation dimension",
                self.backend
            );
        }
        let unenforced = self
            .unenforced()
            .into_iter()
            .map(IsolationDimension::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        write!(formatter, "{} does not enforce: {unenforced}", self.backend)
    }
}

#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub executable: OsString,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub side_effect: SideEffectClass,
    pub stdin: Option<Vec<u8>>,
    pub environment: Vec<(OsString, OsString)>,
}

impl CommandSpec {
    #[must_use]
    pub fn new(
        executable: impl Into<OsString>,
        cwd: impl Into<PathBuf>,
        side_effect: SideEffectClass,
    ) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            side_effect,
            stdin: None,
            environment: Vec::new(),
        }
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn stdin(mut self, bytes: Vec<u8>) -> Self {
        self.stdin = Some(bytes);
        self
    }

    #[must_use]
    pub fn env(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.push((name.into(), value.into()));
        self
    }
}

#[derive(Clone, Debug)]
pub struct ExecutionPolicy {
    workspace_root: PathBuf,
    allowed_side_effects: Vec<SideEffectClass>,
    timeout: Duration,
    max_capture_bytes: usize,
    inherited_environment: Vec<OsString>,
    require_strong_isolation: bool,
    trust_level: TrustLevel,
    network: NetworkClass,
}

impl ExecutionPolicy {
    pub fn trusted_workspace(
        workspace_root: impl AsRef<Path>,
        timeout: Duration,
        max_capture_bytes: usize,
    ) -> Result<Self, ExecutionError> {
        if timeout.is_zero() || max_capture_bytes == 0 {
            return Err(ExecutionError::InvalidPolicy);
        }
        let workspace_root = workspace_root
            .as_ref()
            .canonicalize()
            .map_err(ExecutionError::Io)?;
        Ok(Self {
            workspace_root,
            allowed_side_effects: vec![
                SideEffectClass::PureRead,
                SideEffectClass::WorkspaceWrite,
                SideEffectClass::ProcessExecution,
            ],
            timeout,
            max_capture_bytes,
            inherited_environment: safe_environment_names(),
            require_strong_isolation: false,
            trust_level: TrustLevel::WorkspaceWrite,
            network: NetworkClass::Unrestricted,
        })
    }

    /// A policy for execution a model can influence.
    ///
    /// Unlike [`Self::trusted_workspace`], whose argv AER builds itself, this
    /// profile assumes the command is model-directed and therefore demands a
    /// substrate that enforces every isolation dimension. It exists so the
    /// authority is refused explicitly rather than granted by default by
    /// whatever substrate happens to be available.
    ///
    /// # Errors
    ///
    /// Fails when the bounds are degenerate or the workspace cannot be resolved.
    pub fn sandboxed(
        trust_level: TrustLevel,
        network: NetworkClass,
        workspace_root: impl AsRef<Path>,
        timeout: Duration,
        max_capture_bytes: usize,
    ) -> Result<Self, ExecutionError> {
        let mut policy = Self::trusted_workspace(workspace_root, timeout, max_capture_bytes)?;
        policy.allowed_side_effects = trust_level.side_effects();
        policy.require_strong_isolation = true;
        policy.trust_level = trust_level;
        policy.network = network;
        Ok(policy)
    }

    /// The profile this run is authorized under.
    #[must_use]
    pub const fn trust_level(&self) -> TrustLevel {
        self.trust_level
    }

    /// The network classification this run was created with.
    #[must_use]
    pub const fn network(&self) -> NetworkClass {
        self.network
    }

    #[must_use]
    pub fn allow(mut self, side_effect: SideEffectClass) -> Self {
        if !self.allowed_side_effects.contains(&side_effect) {
            self.allowed_side_effects.push(side_effect);
        }
        self
    }

    #[must_use]
    pub fn require_strong_isolation(mut self, required: bool) -> Self {
        self.require_strong_isolation = required;
        self
    }

    #[must_use]
    pub fn with_inherited_environment<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.inherited_environment = names.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedStream {
    pub preview: Vec<u8>,
    pub sha256: String,
    pub total_bytes: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessResult {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub timed_out: bool,
    pub duration_ms: u128,
    pub stdout: CapturedStream,
    pub stderr: CapturedStream,
    pub security_profile: SecurityProfile,
}

#[derive(Default)]
pub struct LocalProcessExecutor;

impl LocalProcessExecutor {
    pub fn execute(
        &self,
        policy: &ExecutionPolicy,
        spec: CommandSpec,
    ) -> Result<ProcessResult, ExecutionError> {
        if policy.require_strong_isolation {
            return Err(ExecutionError::StrongIsolationUnavailable {
                trust_level: policy.trust_level,
                network: policy.network,
                report: IsolationReport::host_process(),
            });
        }
        if !policy.allowed_side_effects.contains(&spec.side_effect) {
            return Err(ExecutionError::SideEffectDenied(spec.side_effect));
        }
        if !matches!(
            spec.side_effect,
            SideEffectClass::PureRead
                | SideEffectClass::WorkspaceWrite
                | SideEffectClass::ProcessExecution
        ) {
            return Err(ExecutionError::UnsupportedAuthority(spec.side_effect));
        }

        let cwd = spec.cwd.canonicalize().map_err(ExecutionError::Io)?;
        if !path_is_within(policy.workspace_root(), &cwd) {
            return Err(ExecutionError::CwdOutsideWorkspace(cwd));
        }

        let mut command = Command::new(&spec.executable);
        command
            .args(&spec.args)
            .current_dir(&cwd)
            .env_clear()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(if spec.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            });

        for name in &policy.inherited_environment {
            if let Some(value) = env::var_os(name) {
                command.env(name, value);
            }
        }
        for (name, value) in &spec.environment {
            command.env(name, value);
        }

        let argv = normalized_argv(&spec.executable, &spec.args);
        let started = Instant::now();
        let mut child = command.spawn().map_err(ExecutionError::Io)?;

        let stdout = child
            .stdout
            .take()
            .ok_or(ExecutionError::MissingProcessPipe("stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(ExecutionError::MissingProcessPipe("stderr"))?;
        let capture_limit = policy.max_capture_bytes;
        let stdout_thread = thread::spawn(move || capture_stream(stdout, capture_limit));
        let stderr_thread = thread::spawn(move || capture_stream(stderr, capture_limit));

        let stdin_thread = spec.stdin.map(|bytes| {
            child.stdin.take().map(|mut stdin| {
                thread::spawn(move || -> io::Result<()> {
                    stdin.write_all(&bytes)?;
                    stdin.flush()?;
                    Ok(())
                })
            })
        });

        let (status, timed_out) = wait_with_timeout(&mut child, policy.timeout)?;

        if let Some(Some(handle)) = stdin_thread {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) if timed_out && error.kind() == io::ErrorKind::BrokenPipe => {}
                Ok(Err(error)) => return Err(ExecutionError::Io(error)),
                Err(_) => return Err(ExecutionError::WorkerThreadPanicked("stdin")),
            }
        }

        let stdout = join_capture(stdout_thread, "stdout")?;
        let stderr = join_capture(stderr_thread, "stderr")?;

        Ok(ProcessResult {
            argv,
            cwd,
            exit_code: status.code(),
            success: status.success() && !timed_out,
            timed_out,
            duration_ms: started.elapsed().as_millis(),
            stdout,
            stderr,
            security_profile: SecurityProfile::DirectHostProcess,
        })
    }
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<(ExitStatus, bool), ExecutionError> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(ExecutionError::Io)? {
            return Ok((status, false));
        }
        if started.elapsed() >= timeout {
            match child.kill() {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
                Err(error) => return Err(ExecutionError::Io(error)),
            }
            let status = child.wait().map_err(ExecutionError::Io)?;
            return Ok((status, true));
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

fn capture_stream(mut reader: impl Read, limit: usize) -> io::Result<CapturedStream> {
    let mut hasher = Sha256::new();
    let mut preview = Vec::with_capacity(limit.min(16 * 1024));
    let mut total_bytes = 0_u64;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        total_bytes = total_bytes.saturating_add(count as u64);
        if preview.len() < limit {
            let remaining = limit - preview.len();
            preview.extend_from_slice(&buffer[..count.min(remaining)]);
        }
    }
    Ok(CapturedStream {
        truncated: total_bytes > preview.len() as u64,
        preview,
        sha256: lowercase_hex(hasher.finalize().as_ref()),
        total_bytes,
    })
}

#[must_use]
pub fn lowercase_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn join_capture(
    handle: thread::JoinHandle<io::Result<CapturedStream>>,
    stream: &'static str,
) -> Result<CapturedStream, ExecutionError> {
    match handle.join() {
        Ok(result) => result.map_err(ExecutionError::Io),
        Err(_) => Err(ExecutionError::WorkerThreadPanicked(stream)),
    }
}

fn normalized_argv(executable: &OsStr, args: &[OsString]) -> Vec<String> {
    std::iter::once(executable.to_string_lossy().into_owned())
        .chain(args.iter().map(|arg| arg.to_string_lossy().into_owned()))
        .collect()
}

fn safe_environment_names() -> Vec<OsString> {
    [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SYSTEMROOT",
        "TEMP",
        "TMP",
        "COMSPEC",
        "SHELL",
        "PATHEXT",
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

#[cfg(not(windows))]
fn path_is_within(root: &Path, candidate: &Path) -> bool {
    candidate.starts_with(root)
}

#[cfg(windows)]
fn path_is_within(root: &Path, candidate: &Path) -> bool {
    let root = root
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let candidate = candidate
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<Vec<_>>();
    candidate.starts_with(&root)
}

#[derive(Debug)]
pub enum ExecutionError {
    InvalidPolicy,
    StrongIsolationUnavailable {
        trust_level: TrustLevel,
        network: NetworkClass,
        report: IsolationReport,
    },
    SideEffectDenied(SideEffectClass),
    UnsupportedAuthority(SideEffectClass),
    CwdOutsideWorkspace(PathBuf),
    MissingProcessPipe(&'static str),
    WorkerThreadPanicked(&'static str),
    Io(io::Error),
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy => {
                formatter.write_str("execution policy must have nonzero timeout/capture limits")
            }
            Self::StrongIsolationUnavailable {
                trust_level,
                network,
                report,
            } => write!(
                formatter,
                "{} execution with network class {} requires strong isolation; {report}",
                trust_level.as_str(),
                network.as_str()
            ),
            Self::SideEffectDenied(class) => {
                write!(
                    formatter,
                    "execution side effect is denied by policy: {class:?}"
                )
            }
            Self::UnsupportedAuthority(class) => write!(
                formatter,
                "direct host-process adapter refuses high-authority side effect: {class:?}"
            ),
            Self::CwdOutsideWorkspace(path) => {
                write!(
                    formatter,
                    "command cwd escapes workspace boundary: {}",
                    path.display()
                )
            }
            Self::MissingProcessPipe(stream) => write!(formatter, "missing child {stream} pipe"),
            Self::WorkerThreadPanicked(stream) => {
                write!(formatter, "{stream} capture worker panicked")
            }
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl Error for ExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::{
        CommandSpec, ExecutionError, ExecutionPolicy, ISOLATION_DIMENSIONS, IsolationReport,
        LocalProcessExecutor, NetworkClass, SecurityProfile, SideEffectClass, TrustLevel,
    };

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "everything-exec-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    #[test]
    fn direct_executor_captures_trusted_command_without_shell() {
        let root = temp_dir("capture");
        let policy =
            ExecutionPolicy::trusted_workspace(&root, std::time::Duration::from_secs(5), 4096)
                .expect("policy");
        let spec = CommandSpec::new("git", &root, SideEffectClass::PureRead).args(["--version"]);
        let result = LocalProcessExecutor
            .execute(&policy, spec)
            .expect("execute git");
        assert!(result.success);
        assert!(!result.timed_out);
        assert_eq!(result.security_profile, SecurityProfile::DirectHostProcess);
        assert!(String::from_utf8_lossy(&result.stdout.preview).contains("git version"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn cwd_escape_is_rejected_before_spawn() {
        let root = temp_dir("root");
        let outside = temp_dir("outside");
        let policy =
            ExecutionPolicy::trusted_workspace(&root, std::time::Duration::from_secs(1), 1024)
                .expect("policy");
        let spec = CommandSpec::new("git", &outside, SideEffectClass::PureRead).args(["--version"]);
        assert!(matches!(
            LocalProcessExecutor.execute(&policy, spec),
            Err(ExecutionError::CwdOutsideWorkspace(_))
        ));
        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(outside).expect("cleanup outside");
    }

    #[test]
    fn strong_isolation_requirement_fails_closed() {
        let root = temp_dir("isolation");
        let policy =
            ExecutionPolicy::trusted_workspace(&root, std::time::Duration::from_secs(1), 1024)
                .expect("policy")
                .require_strong_isolation(true);
        let spec = CommandSpec::new("git", &root, SideEffectClass::PureRead).args(["--version"]);
        assert!(matches!(
            LocalProcessExecutor.execute(&policy, spec),
            Err(ExecutionError::StrongIsolationUnavailable { .. })
        ));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_model_directed_profile_is_refused_by_the_host_substrate() {
        let root = temp_dir("sandboxed");
        let policy = ExecutionPolicy::sandboxed(
            TrustLevel::WorkspaceWrite,
            NetworkClass::None,
            &root,
            std::time::Duration::from_secs(1),
            1024,
        )
        .expect("policy");
        assert_eq!(policy.trust_level(), TrustLevel::WorkspaceWrite);
        assert_eq!(policy.network(), NetworkClass::None);

        let spec =
            CommandSpec::new("git", &root, SideEffectClass::ProcessExecution).args(["--version"]);
        let error = LocalProcessExecutor
            .execute(&policy, spec)
            .expect_err("host substrate must refuse a model-directed profile");
        let message = error.to_string();
        assert!(message.contains("workspace-write"), "{message}");
        assert!(message.contains("network class none"), "{message}");
        for dimension in ISOLATION_DIMENSIONS {
            assert!(
                message.contains(dimension.as_str()),
                "refusal must name the unenforced `{}` dimension: {message}",
                dimension.as_str()
            );
        }
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn the_host_substrate_claims_no_isolation_dimension() {
        let report = IsolationReport::host_process();
        assert!(!report.is_strong());
        assert_eq!(report.unenforced(), ISOLATION_DIMENSIONS.to_vec());
        for dimension in ISOLATION_DIMENSIONS {
            assert!(
                !report.enforces(dimension),
                "host process cannot enforce {dimension:?}"
            );
        }
        assert_eq!(report.backend(), "direct-host-process");
    }

    #[test]
    fn a_read_only_profile_authorizes_no_write_or_execution() {
        assert_eq!(
            TrustLevel::ReadOnly.side_effects(),
            vec![SideEffectClass::PureRead]
        );
        assert!(
            TrustLevel::WorkspaceWrite
                .side_effects()
                .contains(&SideEffectClass::ProcessExecution)
        );
    }

    #[test]
    fn aer_authored_commands_keep_running_on_the_host_substrate() {
        let root = temp_dir("trusted-still-runs");
        let policy =
            ExecutionPolicy::trusted_workspace(&root, std::time::Duration::from_secs(30), 4096)
                .expect("policy");
        assert_eq!(policy.trust_level(), TrustLevel::WorkspaceWrite);
        assert_eq!(policy.network(), NetworkClass::Unrestricted);
        let spec = CommandSpec::new("git", &root, SideEffectClass::PureRead).args(["--version"]);
        let result = LocalProcessExecutor
            .execute(&policy, spec)
            .expect("AER-authored git invocation must remain available");
        assert!(result.success, "{result:?}");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn high_authority_side_effect_is_refused_even_if_requested() {
        let root = temp_dir("authority");
        let policy =
            ExecutionPolicy::trusted_workspace(&root, std::time::Duration::from_secs(1), 1024)
                .expect("policy")
                .allow(SideEffectClass::CredentialUse);
        let spec =
            CommandSpec::new("git", &root, SideEffectClass::CredentialUse).args(["--version"]);
        assert!(matches!(
            LocalProcessExecutor.execute(&policy, spec),
            Err(ExecutionError::UnsupportedAuthority(
                SideEffectClass::CredentialUse
            ))
        ));
        fs::remove_dir_all(root).expect("cleanup");
    }
}
