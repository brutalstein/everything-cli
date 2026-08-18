use std::{
    error::Error,
    ffi::OsString,
    fmt, fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::Duration,
};

use aer_exec::{
    CommandSpec, ExecutionError, ExecutionPolicy, LocalProcessExecutor, NetworkClass,
    SideEffectClass, TrustLevel,
};
use aer_workspace::OwnedWorktree;
use sha2::{Digest, Sha256};

use crate::permissions::{PermissionController, PermissionDecision, PermissionRequest};

const MAX_READ_LINES: usize = 400;
const MAX_READ_BYTES: usize = 256 * 1024;
const MAX_LIST_ENTRIES: usize = 500;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const COMMAND_CAPTURE_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolDescriptor {
    pub id: &'static str,
    pub summary: &'static str,
    pub side_effect: SideEffectClass,
    pub schema: &'static str,
    /// Why this tool cannot currently run, or `None` when it can.
    ///
    /// A catalog that advertises a capability the runtime always refuses is a
    /// lie told to whatever reads it, model or human. The reason travels with
    /// the descriptor so the refusal is discoverable before the call.
    pub unavailable_because: Option<&'static str>,
}

/// Why `exec.run` is listed but refuses every call.
///
/// Model-directed process execution is the authority that most needs a real
/// isolation boundary, and the only substrate available today is a direct host
/// process, which enforces none of the required dimensions.
const EXEC_RUN_UNAVAILABLE: &str = "model-directed process execution requires a substrate that enforces every isolation \
dimension; the only available substrate is a direct host process, so this tool fails closed";

const TOOLS: [ToolDescriptor; 5] = [
    ToolDescriptor {
        id: "fs.read",
        summary: "Read a bounded UTF-8 line range from one workspace file",
        side_effect: SideEffectClass::PureRead,
        schema: r#"{"path":"string","start_line":"u32?","end_line":"u32?"}"#,
        unavailable_because: None,
    },
    ToolDescriptor {
        id: "fs.list",
        summary: "List a bounded workspace directory deterministically",
        side_effect: SideEffectClass::PureRead,
        schema: r#"{"path":"string?","limit":"u32?"}"#,
        unavailable_because: None,
    },
    ToolDescriptor {
        id: "exec.run",
        summary: "Run one structured argv command inside the current workspace",
        side_effect: SideEffectClass::ProcessExecution,
        schema: r#"{"program":"string","args":"string[]","cwd":"string?","reason":"string"}"#,
        unavailable_because: Some(EXEC_RUN_UNAVAILABLE),
    },
    ToolDescriptor {
        id: "tool.search",
        summary: "Search concise tool metadata without loading every tool schema",
        side_effect: SideEffectClass::PureRead,
        schema: r#"{"query":"string","limit":"u32?"}"#,
        unavailable_because: None,
    },
    ToolDescriptor {
        id: "tool.describe",
        summary: "Return the full schema for one selected tool",
        side_effect: SideEffectClass::PureRead,
        schema: r#"{"tool_id":"string"}"#,
        unavailable_because: None,
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolCall {
    FileRead {
        path: String,
        start_line: Option<u32>,
        end_line: Option<u32>,
    },
    FileList {
        path: Option<String>,
        limit: Option<u32>,
    },
    ExecRun {
        program: String,
        args: Vec<String>,
        cwd: Option<String>,
        reason: String,
    },
    ToolSearch {
        query: String,
        limit: Option<u32>,
    },
    ToolDescribe {
        tool_id: String,
    },
}

impl ToolCall {
    #[must_use]
    pub const fn tool_id(&self) -> &'static str {
        match self {
            Self::FileRead { .. } => "fs.read",
            Self::FileList { .. } => "fs.list",
            Self::ExecRun { .. } => "exec.run",
            Self::ToolSearch { .. } => "tool.search",
            Self::ToolDescribe { .. } => "tool.describe",
        }
    }

    fn permission_request(&self) -> PermissionRequest {
        match self {
            Self::FileRead { path, .. } => {
                PermissionRequest::new(SideEffectClass::PureRead, path, "read workspace file", true)
            }
            Self::FileList { path, .. } => PermissionRequest::new(
                SideEffectClass::PureRead,
                path.as_deref().unwrap_or("."),
                "list workspace directory",
                true,
            ),
            Self::ExecRun {
                program,
                args,
                cwd,
                reason,
            } => PermissionRequest::new(
                SideEffectClass::ProcessExecution,
                format!(
                    "{}{} @ {}",
                    program,
                    if args.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", args.join(" "))
                    },
                    cwd.as_deref().unwrap_or(".")
                ),
                reason,
                false,
            ),
            Self::ToolSearch { query, .. } => PermissionRequest::new(
                SideEffectClass::PureRead,
                query,
                "search tool catalog",
                true,
            ),
            Self::ToolDescribe { tool_id } => PermissionRequest::new(
                SideEffectClass::PureRead,
                tool_id,
                "describe selected tool schema",
                true,
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolOutcome {
    Completed(ToolResult),
    ApprovalRequired(PermissionRequest),
    Denied(PermissionRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolResult {
    FileRead(FileReadResult),
    FileList(FileListResult),
    Exec(ExecResult),
    ToolSearch(Vec<ToolSummary>),
    ToolDescription(ToolSummary),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileReadResult {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
    pub content_sha256: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileListEntry {
    pub name: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileListResult {
    pub path: String,
    pub entries: Vec<FileListEntry>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecResult {
    pub argv: Vec<String>,
    pub cwd: String,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub timed_out: bool,
    pub duration_ms: u128,
    pub stdout_preview: String,
    pub stdout_sha256: String,
    pub stdout_total_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_preview: String,
    pub stderr_sha256: String,
    pub stderr_total_bytes: u64,
    pub stderr_truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolSummary {
    pub id: String,
    pub summary: String,
    pub side_effect: String,
    pub schema: Option<String>,
    /// Present when the runtime will refuse every call to this tool.
    pub unavailable_because: Option<String>,
}

impl ToolSummary {
    /// Projects a catalog entry, disclosing the schema only when asked.
    ///
    /// Progressive disclosure is the reason this exists: a search result should
    /// not carry every schema, but it must still carry whether the tool works.
    fn project(tool: &ToolDescriptor, include_schema: bool) -> Self {
        Self {
            id: tool.id.to_owned(),
            summary: tool.summary.to_owned(),
            side_effect: format!("{:?}", tool.side_effect),
            schema: include_schema.then(|| tool.schema.to_owned()),
            unavailable_because: tool.unavailable_because.map(str::to_owned),
        }
    }
}

pub struct ToolBroker {
    workspace_root: PathBuf,
    process_execution_authorized: bool,
}

impl ToolBroker {
    /// Creates a broker that can inspect a workspace but cannot execute
    /// processes, regardless of permission mode. This is safe for user-owned
    /// checkout inspection surfaces.
    pub fn read_only(workspace_root: &Path) -> Result<Self, ToolError> {
        Ok(Self {
            workspace_root: workspace_root.canonicalize().map_err(ToolError::Io)?,
            process_execution_authorized: false,
        })
    }

    /// Creates a process-capable broker only from AER's unforgeable owned
    /// worktree handle. Permission decisions still apply independently.
    pub fn for_owned_worktree(worktree: &OwnedWorktree) -> Result<Self, ToolError> {
        Ok(Self {
            workspace_root: worktree.path.canonicalize().map_err(ToolError::Io)?,
            process_execution_authorized: true,
        })
    }

    #[cfg(test)]
    fn with_test_process_authority(workspace_root: &Path) -> Result<Self, ToolError> {
        Ok(Self {
            workspace_root: workspace_root.canonicalize().map_err(ToolError::Io)?,
            process_execution_authorized: true,
        })
    }

    #[must_use]
    pub fn core_catalog() -> Vec<ToolSummary> {
        TOOLS
            .iter()
            .map(|tool| ToolSummary::project(tool, false))
            .collect()
    }

    pub fn execute(
        &self,
        permissions: &PermissionController,
        call: ToolCall,
    ) -> Result<ToolOutcome, ToolError> {
        let request = call.permission_request();
        match permissions.decide(&request) {
            PermissionDecision::Ask => return Ok(ToolOutcome::ApprovalRequired(request)),
            PermissionDecision::Deny => return Ok(ToolOutcome::Denied(request)),
            PermissionDecision::Allow => {}
        }

        let result = match call {
            ToolCall::FileRead {
                path,
                start_line,
                end_line,
            } => ToolResult::FileRead(self.file_read(&path, start_line, end_line)?),
            ToolCall::FileList { path, limit } => {
                ToolResult::FileList(self.file_list(path.as_deref().unwrap_or("."), limit)?)
            }
            ToolCall::ExecRun {
                program,
                args,
                cwd,
                reason: _,
            } => ToolResult::Exec(self.exec_run(&program, &args, cwd.as_deref())?),
            ToolCall::ToolSearch { query, limit } => {
                ToolResult::ToolSearch(tool_search(&query, limit)?)
            }
            ToolCall::ToolDescribe { tool_id } => {
                ToolResult::ToolDescription(tool_describe(&tool_id)?)
            }
        };
        Ok(ToolOutcome::Completed(result))
    }

    fn file_read(
        &self,
        relative: &str,
        start_line: Option<u32>,
        end_line: Option<u32>,
    ) -> Result<FileReadResult, ToolError> {
        let path = self.resolve_existing(relative)?;
        if !path.is_file() {
            return Err(ToolError::NotFile(relative.to_owned()));
        }
        let start = start_line.unwrap_or(1);
        let requested_end = end_line.unwrap_or_else(|| {
            start.saturating_add(u32::try_from(MAX_READ_LINES - 1).expect("read bound fits u32"))
        });
        if start == 0 || requested_end < start {
            return Err(ToolError::InvalidLineRange {
                start,
                end: requested_end,
            });
        }
        let hard_end =
            start.saturating_add(u32::try_from(MAX_READ_LINES - 1).expect("read bound fits u32"));
        let end = requested_end.min(hard_end);
        let file = fs::File::open(&path).map_err(ToolError::Io)?;
        let mut content = String::new();
        let mut actual_end = start.saturating_sub(1);
        let mut truncated = requested_end > hard_end;

        for (index, line) in BufReader::new(file).lines().enumerate() {
            let line_number = u32::try_from(index + 1).map_err(|_| ToolError::FileTooLarge)?;
            if line_number < start {
                continue;
            }
            if line_number > end {
                break;
            }
            let line = line.map_err(ToolError::Io)?;
            let required = line.len().saturating_add(1);
            if content.len().saturating_add(required) > MAX_READ_BYTES {
                truncated = true;
                break;
            }
            content.push_str(&line);
            content.push('\n');
            actual_end = line_number;
        }

        Ok(FileReadResult {
            path: workspace_relative(&self.workspace_root, &path)?,
            start_line: start,
            end_line: actual_end,
            content_sha256: sha256_hex(content.as_bytes()),
            content,
            truncated,
        })
    }

    fn file_list(&self, relative: &str, limit: Option<u32>) -> Result<FileListResult, ToolError> {
        let path = self.resolve_existing(relative)?;
        if !path.is_dir() {
            return Err(ToolError::NotDirectory(relative.to_owned()));
        }
        let requested = limit.unwrap_or(100).max(1);
        let limit = usize::try_from(requested)
            .unwrap_or(usize::MAX)
            .min(MAX_LIST_ENTRIES);
        let mut entries = fs::read_dir(&path)
            .map_err(ToolError::Io)?
            .map(|entry| {
                let entry = entry.map_err(ToolError::Io)?;
                let file_type = entry.file_type().map_err(ToolError::Io)?;
                let kind = if file_type.is_dir() {
                    "dir"
                } else if file_type.is_file() {
                    "file"
                } else if file_type.is_symlink() {
                    "symlink"
                } else {
                    "other"
                };
                Ok(FileListEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    kind,
                })
            })
            .collect::<Result<Vec<_>, ToolError>>()?;
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        let truncated = entries.len() > limit;
        entries.truncate(limit);
        Ok(FileListResult {
            path: workspace_relative(&self.workspace_root, &path)?,
            entries,
            truncated,
        })
    }

    fn exec_run(
        &self,
        program: &str,
        args: &[String],
        cwd: Option<&str>,
    ) -> Result<ExecResult, ToolError> {
        if !self.process_execution_authorized {
            return Err(ToolError::ProcessExecutionRequiresOwnedWorktree);
        }
        if program.trim().is_empty() {
            return Err(ToolError::InvalidProgram);
        }
        let cwd = match cwd {
            Some(relative) => self.resolve_existing(relative)?,
            None => self.workspace_root.clone(),
        };
        if !cwd.is_dir() {
            return Err(ToolError::NotDirectory(cwd.to_string_lossy().into_owned()));
        }
        // Model-directed argv is not AER-authored argv. This path therefore
        // demands a substrate that enforces every isolation dimension, and
        // fails closed while none does, rather than inheriting whatever
        // authority the host process happens to hold.
        let policy = ExecutionPolicy::sandboxed(
            TrustLevel::WorkspaceWrite,
            NetworkClass::None,
            &self.workspace_root,
            COMMAND_TIMEOUT,
            COMMAND_CAPTURE_BYTES,
        )?;
        let spec = CommandSpec::new(program, &cwd, SideEffectClass::ProcessExecution)
            .args(args.iter().map(OsString::from));
        let process = LocalProcessExecutor.execute(&policy, spec)?;
        Ok(ExecResult {
            argv: process.argv,
            cwd: workspace_relative(&self.workspace_root, &process.cwd)?,
            exit_code: process.exit_code,
            success: process.success,
            timed_out: process.timed_out,
            duration_ms: process.duration_ms,
            stdout_preview: String::from_utf8_lossy(&process.stdout.preview).into_owned(),
            stdout_sha256: process.stdout.sha256,
            stdout_total_bytes: process.stdout.total_bytes,
            stdout_truncated: process.stdout.truncated,
            stderr_preview: String::from_utf8_lossy(&process.stderr.preview).into_owned(),
            stderr_sha256: process.stderr.sha256,
            stderr_total_bytes: process.stderr.total_bytes,
            stderr_truncated: process.stderr.truncated,
        })
    }

    fn resolve_existing(&self, relative: &str) -> Result<PathBuf, ToolError> {
        let raw = Path::new(relative);
        if raw.is_absolute() {
            return Err(ToolError::AbsolutePathDenied(relative.to_owned()));
        }
        let candidate = self.workspace_root.join(raw);
        let canonical = candidate.canonicalize().map_err(ToolError::Io)?;
        if !canonical.starts_with(&self.workspace_root) {
            return Err(ToolError::WorkspaceEscape(canonical));
        }
        Ok(canonical)
    }
}

fn tool_search(query: &str, limit: Option<u32>) -> Result<Vec<ToolSummary>, ToolError> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Err(ToolError::EmptyToolQuery);
    }
    let limit = usize::try_from(limit.unwrap_or(8).max(1))
        .unwrap_or(usize::MAX)
        .min(TOOLS.len());
    Ok(TOOLS
        .iter()
        .filter(|tool| {
            tool.id.to_ascii_lowercase().contains(&query)
                || tool.summary.to_ascii_lowercase().contains(&query)
        })
        .take(limit)
        .map(|tool| ToolSummary::project(tool, false))
        .collect())
}

fn tool_describe(tool_id: &str) -> Result<ToolSummary, ToolError> {
    let tool = TOOLS
        .iter()
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| ToolError::UnknownTool(tool_id.to_owned()))?;
    Ok(ToolSummary::project(tool, true))
}

fn workspace_relative(root: &Path, path: &Path) -> Result<String, ToolError> {
    path.strip_prefix(root)
        .map_err(|_| ToolError::WorkspaceEscape(path.to_path_buf()))
        .map(|relative| {
            if relative.as_os_str().is_empty() {
                ".".to_owned()
            } else {
                relative.to_string_lossy().replace('\\', "/")
            }
        })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[derive(Debug)]
pub enum ToolError {
    Io(std::io::Error),
    Execution(ExecutionError),
    AbsolutePathDenied(String),
    WorkspaceEscape(PathBuf),
    NotFile(String),
    NotDirectory(String),
    InvalidLineRange { start: u32, end: u32 },
    FileTooLarge,
    ProcessExecutionRequiresOwnedWorktree,
    InvalidProgram,
    EmptyToolQuery,
    UnknownTool(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Execution(error) => error.fmt(formatter),
            Self::AbsolutePathDenied(path) => {
                write!(formatter, "absolute tool path is denied: {path}")
            }
            Self::WorkspaceEscape(path) => {
                write!(formatter, "tool path escaped workspace: {}", path.display())
            }
            Self::NotFile(path) => write!(formatter, "tool target is not a file: {path}"),
            Self::NotDirectory(path) => {
                write!(formatter, "tool target is not a directory: {path}")
            }
            Self::InvalidLineRange { start, end } => {
                write!(formatter, "invalid file line range {start}..={end}")
            }
            Self::FileTooLarge => formatter.write_str("file has more addressable lines than u32"),
            Self::ProcessExecutionRequiresOwnedWorktree => {
                formatter.write_str("exec.run requires an AER-owned worktree authority token")
            }
            Self::InvalidProgram => formatter.write_str("exec.run program cannot be empty"),
            Self::EmptyToolQuery => formatter.write_str("tool.search query cannot be empty"),
            Self::UnknownTool(tool) => write!(formatter, "unknown tool `{tool}`"),
        }
    }
}

impl Error for ToolError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Execution(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ExecutionError> for ToolError {
    fn from(value: ExecutionError) -> Self {
        Self::Execution(value)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use aer_exec::SideEffectClass;

    use crate::permissions::{PermissionController, PermissionMode};

    use super::{ToolBroker, ToolCall, ToolOutcome, ToolResult};

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> std::path::PathBuf {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "aer-tools-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos(),
            sequence
        ));
        fs::create_dir_all(root.join("src")).expect("fixture dirs");
        fs::write(root.join("src/lib.rs"), "one\ntwo\nthree\nfour\n").expect("fixture file");
        root
    }

    fn cleanup_fixture(root: std::path::PathBuf, broker: ToolBroker) {
        drop(broker);
        let mut last_error = None;
        for attempt in 0..10 {
            match fs::remove_dir_all(&root) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 9 {
                        thread::sleep(Duration::from_millis(25));
                    }
                }
            }
        }
        panic!("cleanup: {}", last_error.expect("cleanup error"));
    }

    #[test]
    fn default_mode_auto_reads_but_returns_typed_approval_for_commands() {
        let root = fixture();
        let broker = ToolBroker::read_only(&root).expect("broker");
        let permissions = PermissionController::developer_workspace(PermissionMode::Default);
        let read = broker
            .execute(
                &permissions,
                ToolCall::FileRead {
                    path: "src/lib.rs".to_owned(),
                    start_line: Some(2),
                    end_line: Some(3),
                },
            )
            .expect("read");
        let ToolOutcome::Completed(ToolResult::FileRead(read)) = read else {
            panic!("expected completed read");
        };
        assert_eq!(read.content, "two\nthree\n");
        assert_eq!(read.start_line, 2);
        assert_eq!(read.end_line, 3);

        let exec = broker
            .execute(
                &permissions,
                ToolCall::ExecRun {
                    program: "git".to_owned(),
                    args: vec!["status".to_owned(), "--short".to_owned()],
                    cwd: None,
                    reason: "inspect status".to_owned(),
                },
            )
            .expect("exec decision");
        let ToolOutcome::ApprovalRequired(request) = exec else {
            panic!("default exec must ask");
        };
        assert_eq!(request.side_effect, SideEffectClass::ProcessExecution);
        cleanup_fixture(root, broker);
    }

    #[test]
    fn auto_mode_cannot_execute_without_owned_worktree_authority() {
        let root = fixture();
        let broker = ToolBroker::read_only(&root).expect("broker");
        let permissions = PermissionController::developer_workspace(PermissionMode::Auto);
        let error = broker
            .execute(
                &permissions,
                ToolCall::ExecRun {
                    program: "git".to_owned(),
                    args: vec!["--version".to_owned()],
                    cwd: None,
                    reason: "attempt process without owned worktree".to_owned(),
                },
            )
            .expect_err("process execution must require owned-worktree authority");
        assert!(matches!(
            error,
            super::ToolError::ProcessExecutionRequiresOwnedWorktree
        ));
        cleanup_fixture(root, broker);
    }

    #[test]
    fn even_full_authority_cannot_execute_without_an_isolating_substrate() {
        let root = fixture();
        let broker = ToolBroker::with_test_process_authority(&root).expect("broker");
        // Every gate above the sandbox is deliberately wide open here: owned
        // worktree authority, the most permissive permission mode, and a
        // structured argv. The refusal must come from the substrate alone.
        let permissions = PermissionController::developer_workspace(PermissionMode::Full);
        let error = broker
            .execute(
                &permissions,
                ToolCall::ExecRun {
                    program: "git".to_owned(),
                    args: vec!["--version".to_owned()],
                    cwd: None,
                    reason: "verify structured command transport".to_owned(),
                },
            )
            .expect_err("no substrate enforces the required isolation dimensions");
        let message = error.to_string();
        assert!(message.contains("strong isolation"), "{message}");
        assert!(message.contains("workspace-write"), "{message}");
        cleanup_fixture(root, broker);
    }

    #[test]
    fn the_catalog_states_that_process_execution_is_unavailable() {
        let catalog = ToolBroker::core_catalog();
        let exec = catalog
            .iter()
            .find(|tool| tool.id == "exec.run")
            .expect("exec.run stays discoverable");
        assert!(
            exec.unavailable_because.is_some(),
            "a tool that always refuses must say so before it is called"
        );
        for tool in catalog.iter().filter(|tool| tool.id != "exec.run") {
            assert!(
                tool.unavailable_because.is_none(),
                "{} is available and must not claim otherwise",
                tool.id
            );
        }
    }

    #[test]
    fn tool_catalog_is_progressively_disclosed() {
        let catalog = ToolBroker::core_catalog();
        assert!(catalog.iter().all(|tool| tool.schema.is_none()));
        let root = fixture();
        let broker = ToolBroker::read_only(&root).expect("broker");
        let permissions = PermissionController::developer_workspace(PermissionMode::Default);
        let result = broker
            .execute(
                &permissions,
                ToolCall::ToolDescribe {
                    tool_id: "exec.run".to_owned(),
                },
            )
            .expect("describe");
        let ToolOutcome::Completed(ToolResult::ToolDescription(tool)) = result else {
            panic!("expected tool description");
        };
        assert!(tool.schema.is_some());
        cleanup_fixture(root, broker);
    }

    #[test]
    fn plan_mode_denies_command_even_when_the_program_is_structured() {
        let root = fixture();
        let broker = ToolBroker::read_only(&root).expect("broker");
        let permissions = PermissionController::developer_workspace(PermissionMode::Plan);
        let result = broker
            .execute(
                &permissions,
                ToolCall::ExecRun {
                    program: "git".to_owned(),
                    args: vec!["status".to_owned()],
                    cwd: None,
                    reason: "inspect".to_owned(),
                },
            )
            .expect("decision");
        assert!(matches!(result, ToolOutcome::Denied(_)));
        cleanup_fixture(root, broker);
    }
}
