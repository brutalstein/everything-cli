//! Bounded subprocess execution for provider measurement.
//!
//! Every harness call is a real vendor CLI invocation, so the runner must be
//! deterministic across machines: the child inherits an explicit allowlist of
//! environment variables rather than the operator's shell, argv is constructed
//! by the harness alone, and both output streams are capped and timed.

use std::{
    env,
    ffi::{OsStr, OsString},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{HarnessError, preview};

/// Environment the child is allowed to see.
///
/// Everything else is cleared. Delegated authentication still works because the
/// vendor stores its session under the home/config directories named here; no
/// credential is read, copied or logged by the harness itself.
const INHERITED: [&str; 19] = [
    "PATH",
    "PATHEXT",
    "HOME",
    "USERPROFILE",
    "SYSTEMROOT",
    "COMSPEC",
    "APPDATA",
    "LOCALAPPDATA",
    "TEMP",
    "TMP",
    "SHELL",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CODE_GIT_BASH_PATH",
    "LANG",
    "LC_ALL",
    "TERM",
    "NO_COLOR",
];

/// A completed child process.
#[derive(Debug)]
pub struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
    pub duration: Duration,
}

/// One bounded invocation.
pub struct ProcessSpec<'a> {
    pub executable: &'a Path,
    pub args: &'a [OsString],
    pub cwd: &'a Path,
    pub stdin: &'a [u8],
    /// Extra environment applied after the inherited allowlist.
    pub env: &'a [(&'a str, &'a str)],
    pub timeout: Duration,
    pub max_output: usize,
}

/// Runs one child to completion under an output cap and a wall-clock timeout.
///
/// # Errors
///
/// Returns [`HarnessError::TimedOut`] when the child outlives `timeout`, and an
/// IO error when a pipe or the spawn itself fails.
pub fn run_bounded(spec: &ProcessSpec<'_>) -> Result<ProcessOutput, HarnessError> {
    let mut command = Command::new(spec.executable);
    command
        .args(spec.args)
        .current_dir(spec.cwd)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    inherit_environment(&mut command);
    for (key, value) in spec.env {
        command.env(key, value);
    }

    let mut child = command.spawn()?;
    let mut input = child
        .stdin
        .take()
        .ok_or(HarnessError::MissingPipe("stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or(HarnessError::MissingPipe("stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(HarnessError::MissingPipe("stderr"))?;
    let payload = spec.stdin.to_vec();
    let input_worker = thread::spawn(move || -> io::Result<()> {
        input.write_all(&payload)?;
        input.flush()
    });
    let max_output = spec.max_output;
    let stdout_worker = thread::spawn(move || capture(stdout, max_output));
    let stderr_worker = thread::spawn(move || capture(stderr, max_output));

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= spec.timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let duration = started.elapsed();

    match input_worker.join() {
        Ok(Ok(())) => {}
        // A child that answers before draining stdin, or one we killed, closes
        // the pipe early. That is not a measurement failure.
        Ok(Err(error))
            if error.kind() == io::ErrorKind::BrokenPipe
                || error.kind() == io::ErrorKind::WriteZero => {}
        Ok(Err(error)) => return Err(HarnessError::Io(error)),
        Err(_) => return Err(HarnessError::Worker("stdin")),
    }
    let stdout = join_capture(stdout_worker, "stdout")?;
    let stderr = join_capture(stderr_worker, "stderr")?;
    if timed_out {
        return Err(HarnessError::TimedOut {
            seconds: spec.timeout.as_secs(),
        });
    }
    Ok(ProcessOutput {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        truncated: stdout.truncated || stderr.truncated,
        duration,
    })
}

struct Capture {
    bytes: Vec<u8>,
    truncated: bool,
}

fn capture(mut reader: impl Read, max_output: usize) -> io::Result<Capture> {
    let mut bytes = Vec::with_capacity(64 * 1024);
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = max_output.saturating_sub(bytes.len());
        let keep = count.min(remaining);
        bytes.extend_from_slice(&buffer[..keep]);
        truncated |= keep < count;
    }
    Ok(Capture { bytes, truncated })
}

fn join_capture(
    worker: thread::JoinHandle<io::Result<Capture>>,
    stream: &'static str,
) -> Result<Capture, HarnessError> {
    worker
        .join()
        .map_err(|_| HarnessError::Worker(stream))?
        .map_err(HarnessError::Io)
}

fn inherit_environment(command: &mut Command) {
    for key in INHERITED {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
}

/// Resolves an executable against `PATH`, honouring `PATHEXT` on Windows.
///
/// # Errors
///
/// Returns [`HarnessError::Executable`] when no candidate exists.
pub fn resolve_executable(name: &str) -> Result<PathBuf, HarnessError> {
    let path = env::var_os("PATH").ok_or_else(|| HarnessError::Executable(name.to_owned()))?;
    #[cfg(windows)]
    let suffixes = windows_suffixes(name);
    #[cfg(not(windows))]
    let suffixes = vec![String::new()];
    for directory in env::split_paths(&path) {
        for suffix in &suffixes {
            let candidate = directory.join(format!("{name}{suffix}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(HarnessError::Executable(name.to_owned()))
}

/// Executable suffixes to try on Windows, in `PATHEXT` order and de-duplicated.
#[cfg(windows)]
#[must_use]
pub fn windows_suffixes(name: &str) -> Vec<String> {
    if Path::new(name).extension().is_some() {
        return vec![String::new()];
    }
    let mut values = vec![String::new()];
    let pathext = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());
    for suffix in pathext
        .split(';')
        .filter(|suffix| !suffix.trim().is_empty())
    {
        let suffix = suffix.trim();
        let normalized = if suffix.starts_with('.') {
            suffix.to_owned()
        } else {
            format!(".{suffix}")
        };
        if !values
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&normalized))
        {
            values.push(normalized);
        }
    }
    values
}

/// `<executable> --version`, trimmed.
///
/// # Errors
///
/// Returns [`HarnessError::Version`] when the probe exits non-zero.
pub fn executable_version(executable: &Path) -> Result<String, HarnessError> {
    let output = Command::new(executable).arg("--version").output()?;
    if !output.status.success() {
        return Err(HarnessError::Version(output.status.code()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Renders argv for a receipt without pretending it is a shell command line.
///
/// Arguments are emitted one per line exactly as they are passed to the OS, so
/// a reader can reproduce the call without guessing how a shell would have
/// split or unescaped them.
#[must_use]
pub fn render_argv(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|arg| OsStr::to_string_lossy(arg).into_owned())
        .collect()
}

/// First lines of a child's stderr, for failure receipts.
#[must_use]
pub fn stderr_preview(output: &ProcessOutput) -> String {
    preview(&String::from_utf8_lossy(&output.stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_environment_never_includes_credential_carrying_variables() {
        for key in INHERITED {
            let upper = key.to_ascii_uppercase();
            assert!(!upper.contains("TOKEN"), "{key} may carry a credential");
            assert!(!upper.contains("KEY"), "{key} may carry a credential");
            assert!(!upper.contains("SECRET"), "{key} may carry a credential");
            assert!(!upper.contains("PASSWORD"), "{key} may carry a credential");
        }
    }

    #[test]
    fn argv_is_rendered_one_entry_per_argument_without_shell_quoting() {
        let args = vec![
            OsString::from("-p"),
            OsString::from("a b \"c\" 'd' \\e"),
            OsString::from(""),
        ];
        let rendered = render_argv(&args);
        assert_eq!(
            rendered,
            vec![
                "-p".to_owned(),
                "a b \"c\" 'd' \\e".to_owned(),
                String::new()
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_suffixes_are_unique_and_start_with_the_bare_name() {
        let suffixes = windows_suffixes("claude");
        assert_eq!(suffixes.first().map(String::as_str), Some(""));
        let mut sorted = suffixes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), suffixes.len());
        assert!(
            suffixes
                .iter()
                .any(|suffix| suffix.eq_ignore_ascii_case(".cmd"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn an_explicit_extension_is_not_expanded() {
        assert_eq!(windows_suffixes("claude.exe"), vec![String::new()]);
    }
}
