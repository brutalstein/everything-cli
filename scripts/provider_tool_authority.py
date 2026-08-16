from pathlib import Path
import hashlib

path = Path("crates/aer-core/src/tools.rs")
text = path.read_text(encoding="utf-8")

old = '''use aer_exec::{
    CommandSpec, ExecutionError, ExecutionPolicy, LocalProcessExecutor, SideEffectClass,
};
use sha2::{Digest, Sha256};
'''
new = '''use aer_exec::{
    CommandSpec, ExecutionError, ExecutionPolicy, LocalProcessExecutor, SideEffectClass,
};
use aer_workspace::OwnedWorktree;
use sha2::{Digest, Sha256};
'''
if text.count(old) != 1:
    raise SystemExit("ToolBroker import anchor mismatch")
text = text.replace(old, new, 1)

old = '''pub struct ToolBroker {
    workspace_root: PathBuf,
}

impl ToolBroker {
    pub fn new(workspace_root: &Path) -> Result<Self, ToolError> {
        Ok(Self {
            workspace_root: workspace_root.canonicalize().map_err(ToolError::Io)?,
        })
    }
'''
new = '''pub struct ToolBroker {
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
'''
if text.count(old) != 1:
    raise SystemExit("ToolBroker constructor anchor mismatch")
text = text.replace(old, new, 1)

old = '''    ) -> Result<ExecResult, ToolError> {
        if program.trim().is_empty() {
            return Err(ToolError::InvalidProgram);
        }
'''
new = '''    ) -> Result<ExecResult, ToolError> {
        if !self.process_execution_authorized {
            return Err(ToolError::ProcessExecutionRequiresOwnedWorktree);
        }
        if program.trim().is_empty() {
            return Err(ToolError::InvalidProgram);
        }
'''
if text.count(old) != 1:
    raise SystemExit("ToolBroker exec authority anchor mismatch")
text = text.replace(old, new, 1)

old = '''    FileTooLarge,
    InvalidProgram,
    EmptyToolQuery,
'''
new = '''    FileTooLarge,
    ProcessExecutionRequiresOwnedWorktree,
    InvalidProgram,
    EmptyToolQuery,
'''
if text.count(old) != 1:
    raise SystemExit("ToolError variant anchor mismatch")
text = text.replace(old, new, 1)

old = '''            Self::FileTooLarge => formatter.write_str("file has more addressable lines than u32"),
            Self::InvalidProgram => formatter.write_str("exec.run program cannot be empty"),
'''
new = '''            Self::FileTooLarge => formatter.write_str("file has more addressable lines than u32"),
            Self::ProcessExecutionRequiresOwnedWorktree => formatter.write_str(
                "exec.run requires an AER-owned worktree authority token",
            ),
            Self::InvalidProgram => formatter.write_str("exec.run program cannot be empty"),
'''
if text.count(old) != 1:
    raise SystemExit("ToolError display anchor mismatch")
text = text.replace(old, new, 1)

text = text.replace("ToolBroker::new(&root)", "ToolBroker::read_only(&root)")
auto_anchor = '''    fn auto_mode_executes_structured_local_command_with_bounded_evidence() {
        let root = fixture();
        let broker = ToolBroker::read_only(&root).expect("broker");
'''
auto_new = '''    fn auto_mode_executes_structured_local_command_with_bounded_evidence() {
        let root = fixture();
        let broker = ToolBroker::with_test_process_authority(&root).expect("broker");
'''
if text.count(auto_anchor) != 1:
    raise SystemExit("auto exec fixture anchor mismatch")
text = text.replace(auto_anchor, auto_new, 1)

insert_anchor = '''    #[test]
    fn auto_mode_executes_structured_local_command_with_bounded_evidence() {
'''
extra = '''    #[test]
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
        fs::remove_dir_all(root).expect("cleanup");
    }

'''
if extra not in text:
    if text.count(insert_anchor) != 1:
        raise SystemExit("owned-worktree test insertion anchor mismatch")
    text = text.replace(insert_anchor, extra + insert_anchor, 1)
path.write_text(text, encoding="utf-8")

spec = Path("docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md")
text = spec.read_text(encoding="utf-8")
needle = "6. **One write authority.** Workspace mutation occurs only in AER-owned isolated worktrees."
replacement = "6. **One write authority.** Workspace mutation occurs only in AER-owned isolated worktrees. Process-capable `ToolBroker` construction requires an `aer_workspace::OwnedWorktree` authority token; permission mode alone cannot authorize commands in a user-owned checkout."
if needle in text:
    text = text.replace(needle, replacement, 1)
elif replacement not in text:
    raise SystemExit("docs45 one-write-authority anchor mismatch")
spec.write_text(text, encoding="utf-8")

status = Path("STATUS.md")
text = status.read_text(encoding="utf-8")
needle = '| Structured `exec.run` command evidence | PASS | Auto-mode real `git --version` ToolBroker test. |'
addition = needle + '\n| Process execution requires AER-owned worktree authority | PASS | production constructor requires `OwnedWorktree`; read-only broker fails closed even in Auto mode. |'
if addition not in text:
    if text.count(needle) != 1:
        raise SystemExit("STATUS ToolBroker ledger anchor mismatch")
    text = text.replace(needle, addition, 1)
status.write_text(text, encoding="utf-8")

# Refresh docs hashes after the normative security clarification.
docs = Path("docs")
manifest = docs / "MANIFEST.sha256"
entries = []
for doc in sorted(docs.rglob("*")):
    if not doc.is_file() or doc == manifest:
        continue
    entries.append(f"{hashlib.sha256(doc.read_bytes()).hexdigest()}  {doc.as_posix()}")
manifest.write_text("\n".join(entries) + "\n", encoding="utf-8")
