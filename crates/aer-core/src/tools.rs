use std::{
    error::Error,
    ffi::OsString,
    fmt,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::Duration,
};

use aer_exec::{
    CommandSpec, ExecutionError, ExecutionPolicy, LocalProcessExecutor, SideEffectClass,
};
use sha2::{Digest, Sha256};

use crate::permissions::{
    PermissionController, PermissionDecision, PermissionRequest,
};

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
}

const TOOLS: [ToolDescriptor; 5] = [
    ToolDescriptor {
        id: "fs.read",
        summary: "Read a bounded UTF-8 line range from one workspace file",
        side_effect: SideEffectClass::PureRead,
        schema: r#"{"path":"string","start_line":"u32?","end_line":"u32?"}"#,
    },
    ToolDescriptor {
        id: "fs.list",
        summary: "List a bounded workspace directory deterministically",
        side_effect: SideEffectClass::PureRead,
        schema: r#"{"path":"string?","limit":"u32?"}"#,
    },
    ToolDescriptor {
        id: "exec.run",
        summary: "Run one structured argv command inside the current workspace",
        side_effect: SideEffectClass::ProcessExecution,
        schema: r#"{"program":"string","args":"string[]","cwd":"string?","reason":"string"}"#,
    },
    ToolDescriptor {
        id: "tool.search",
        summary: "Search concise tool metadata without loading every tool schema",
        side_effect: SideEffectClass::PureRead,
        schema: r#"{"query":"string","limit":"u32?"}"#,
    },
    ToolDescriptor {
        id: "tool.describe",
        summary: "Return the full schema for one selected tool",
        side_effect: SideEffectClass::PureRead,
        schema: r#"{"tool_id":"string"}"#,
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
            Self::FileRead { path, .. } => PermissionRequest::new(
                SideEffectClass::PureRead,
                path,
                "read workspace file",
                true,
            ),
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
}

pub struct ToolBroker {
    workspace_root: PathBuf,
}

impl ToolBroker {
    pub fn new(workspace_root: &Path) -> Result<Self, ToolError> {
        Ok(Self {
            workspace_root: workspace_root.canonicalize().map_err(ToolError::Io)?,
        })
    }

    #[must_use]
    pub fn core_catalog() -> Vec<ToolSummary> {
        TOOLS
            .iter()
            .map(|tool| ToolSummary {
                id: tool.id.to_owned(),
                summary: tool.summary.to_owned(),
                side_effect: format!("{:?}", tool.side_effect),
                schema: None,
            })
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
        let hard_end = start.saturating_add(
            u32::try_from(MAX_READ_LINES - 1).expect("read bound fits u32"),
        );
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
        if program.trim().is_empty() {
            return Err(ToolError::InvalidProgram);
        }
        let cwd = match cwd {
            Some(relative) => self.resolve_existing(relative)?,
            None => self.workspace_root.clone(),
        };
        if !cwd.is_dir() {
            return Err(ToolError::NotDirectory(
                cwd.to_string_lossy().into_owned(),
            ));
        }
        let policy = ExecutionPolicy::trusted_workspace(
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
        .map(|tool| ToolSummary {
            id: tool.id.to_owned(),
            summary: tool.summary.to_owned(),
            side_effect: format!("{:?}", tool.side_effect),
            schema: None,
        })
        .collect())
}

fn tool_describe(tool_id: &str) -> Result<ToolSummary, ToolError> {
    let tool = TOOLS
        .iter()
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| ToolError::UnknownTool(tool_id.to_owned()))?;
    Ok(ToolSummary {
        id: tool.id.to_owned(),
        summary: tool.summary.to_owned(),
        side_effect: format!("{:?}", tool.side_effect),
        schema: Some(tool.schema.to_owned()),
    })
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
    use std::{fs, time::{SystemTime, UNIX_EPOCH}};

    use aer_exec::SideEffectClass;

    use crate::permissions::{PermissionController, PermissionMode};

    use super::{ToolBroker, ToolCall, ToolOutcome, ToolResult};

    fn fixture() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "aer-tools-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).expect("fixture dirs");
        fs::write(root.join("src/lib.rs"), "one\ntwo\nthree\nfour\n").expect("fixture file");
        root
    }

    #[test]
    fn default_mode_auto_reads_but_returns_typed_approval_for_commands() {
        let root = fixture();
        let broker = ToolBroker::new(&root).expect("broker");
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
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn tool_catalog_is_progressively_disclosed() {
        let catalog = ToolBroker::core_catalog();
        assert!(catalog.iter().all(|tool| tool.schema.is_none()));
        let root = fixture();
        let broker = ToolBroker::new(&root).expect("broker");
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
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn plan_mode_denies_command_even_when_the_program_is_structured() {
        let root = fixture();
        let broker = ToolBroker::new(&root).expect("broker");
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
        fs::remove_dir_all(root).expect("cleanup");
    }
}
