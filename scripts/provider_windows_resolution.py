from pathlib import Path

path = Path("crates/aer-provider/src/delegated.rs")
text = path.read_text(encoding="utf-8")

text = text.replace(
    '    path::{Path, PathBuf},\n',
    '    path::{Path, PathBuf},\n',
    1,
)

old = '''    pub fn login(
        kind: DelegatedProviderKind,
        workspace: &Path,
        flow: LoginFlow,
    ) -> Result<(), DelegatedProviderError> {
        let mut command = Command::new(kind.executable());
        command.current_dir(workspace);
'''
new = '''    pub fn login(
        kind: DelegatedProviderKind,
        workspace: &Path,
        flow: LoginFlow,
    ) -> Result<(), DelegatedProviderError> {
        let executable = resolve_executable(kind.executable())?;
        let mut command = Command::new(&executable);
        command.current_dir(workspace);
'''
if text.count(old) != 1:
    raise SystemExit("login executable anchor mismatch")
text = text.replace(old, new, 1)

old = '''        let status = Command::new(kind.executable())
            .args(args)
            .current_dir(workspace)
            .status()
'''
new = '''        let executable = resolve_executable(kind.executable())?;
        let status = Command::new(&executable)
            .args(args)
            .current_dir(workspace)
            .status()
'''
if text.count(old) != 1:
    raise SystemExit("logout executable anchor mismatch")
text = text.replace(old, new, 1)

old = '''fn run_bounded(
    executable: &str,
    args: &[OsString],
    cwd: &Path,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<BoundedProcessResult, DelegatedProviderError> {
    let mut command = Command::new(executable);
'''
new = '''fn run_bounded(
    executable: &str,
    args: &[OsString],
    cwd: &Path,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<BoundedProcessResult, DelegatedProviderError> {
    let resolved_executable = resolve_executable(executable)?;
    let mut command = Command::new(&resolved_executable);
'''
if text.count(old) != 1:
    raise SystemExit("run_bounded executable anchor mismatch")
text = text.replace(old, new, 1)

insert_before = '''fn inherit_safe_provider_environment(command: &mut Command) {
'''
resolver = r'''fn resolve_executable(executable: &str) -> Result<PathBuf, DelegatedProviderError> {
    let direct = Path::new(executable);
    if direct.components().count() > 1 && direct.is_file() {
        return Ok(direct.to_path_buf());
    }
    let path = env::var_os("PATH").ok_or_else(|| DelegatedProviderError::Spawn {
        executable: executable.to_owned(),
        error: io::Error::new(io::ErrorKind::NotFound, "PATH is not set"),
    })?;

    #[cfg(windows)]
    let suffixes = windows_executable_suffixes(executable);
    #[cfg(not(windows))]
    let suffixes = vec![String::new()];

    for directory in env::split_paths(&path) {
        for suffix in &suffixes {
            let candidate = directory.join(format!("{executable}{suffix}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(DelegatedProviderError::Spawn {
        executable: executable.to_owned(),
        error: io::Error::new(io::ErrorKind::NotFound, "provider executable not found on PATH"),
    })
}

#[cfg(windows)]
fn windows_executable_suffixes(executable: &str) -> Vec<String> {
    if Path::new(executable).extension().is_some() {
        return vec![String::new()];
    }
    let mut suffixes = vec![String::new()];
    let pathext = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());
    for suffix in pathext.split(';').filter(|value| !value.trim().is_empty()) {
        let suffix = suffix.trim();
        let normalized = if suffix.starts_with('.') {
            suffix.to_owned()
        } else {
            format!(".{suffix}")
        };
        if !suffixes.iter().any(|existing| existing.eq_ignore_ascii_case(&normalized)) {
            suffixes.push(normalized);
        }
    }
    suffixes
}

'''
if text.count(insert_before) != 1:
    raise SystemExit("environment helper insertion anchor mismatch")
text = text.replace(insert_before, resolver + insert_before, 1)

old = '''        "CLAUDE_CONFIG_DIR",
        "LANG",
'''
new = '''        "CLAUDE_CONFIG_DIR",
        "CLAUDE_CODE_GIT_BASH_PATH",
        "LANG",
'''
if text.count(old) != 1:
    raise SystemExit("safe environment anchor mismatch")
text = text.replace(old, new, 1)

windows_test_anchor = '''    #[test]
    fn single_json_parser_supports_claude_and_gemini_shape() {
'''
windows_test = r'''    #[cfg(windows)]
    #[test]
    fn windows_executable_suffixes_include_cmd_without_duplicates() {
        let suffixes = super::windows_executable_suffixes("claude");
        assert!(suffixes.iter().any(|suffix| suffix.eq_ignore_ascii_case(".cmd")));
        assert_eq!(
            suffixes
                .iter()
                .filter(|suffix| suffix.eq_ignore_ascii_case(".cmd"))
                .count(),
            1
        );
    }

'''
if windows_test not in text:
    if text.count(windows_test_anchor) != 1:
        raise SystemExit("windows resolver test insertion anchor mismatch")
    text = text.replace(windows_test_anchor, windows_test + windows_test_anchor, 1)

path.write_text(text, encoding="utf-8")
