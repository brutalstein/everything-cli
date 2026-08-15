//! Phase-1 single-agent application runtime.
//!
//! The runtime composes durable events, provider normalization, isolated Git
//! worktrees, typed process execution, environment-bound evidence, interruption
//! checkpoints, and deterministic resume. It deliberately does not implement
//! repository retrieval, autonomous verifier synthesis, or parallel scheduling.

use std::{
    collections::BTreeSet,
    error::Error,
    fmt, fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use aer_domain::state_machines::RunState;
use aer_environment::{EnvironmentFingerprint, evidence::CommandExecutionEvidence};
use aer_exec::{
    CommandSpec, ExecutionPolicy, LocalProcessExecutor, SideEffectClass, lowercase_hex,
};
use aer_provider::{CancellationSignal, ProviderAdapter, ProviderGateway, ProviderRequest};
use aer_storage::{
    DurableState, EventPayload, NewEvent, ObjectHash, ObjectMetadata, Sensitivity, StoredEvent,
};
use aer_workspace::{SnapshotPolicy, WorkspaceIdentity, WorkspaceSnapshot};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use ulid::Ulid;

const MAX_PLAN_BYTES: usize = 4 * 1024 * 1024;
const MAX_EDIT_BYTES: usize = 1024 * 1024;
const MAX_EDITS: usize = 16;
const VERIFY_CAPTURE_BYTES: usize = 2 * 1024 * 1024;
const VERIFY_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationCommand {
    pub executable: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedFile {
    pub relative_path: String,
    pub sha256: String,
}

impl ExpectedFile {
    #[must_use]
    pub fn from_bytes(relative_path: impl Into<String>, bytes: &[u8]) -> Self {
        Self {
            relative_path: relative_path.into(),
            sha256: lowercase_hex(Sha256::digest(bytes).as_ref()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationSpec {
    pub command: VerificationCommand,
    pub expected_files: Vec<ExpectedFile>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunRequest {
    pub workspace_root: PathBuf,
    pub state_home: PathBuf,
    pub goal: String,
    pub verification: VerificationSpec,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptAfter {
    ProviderResponse,
    EditsApplied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunSummary {
    pub run_id: String,
    pub project_id: String,
    pub state: RunState,
    pub goal: String,
    pub worktree_path: PathBuf,
    pub provider_attempts: u32,
    pub verification_success: Option<bool>,
    pub accepted: bool,
    pub interrupted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileEdit {
    relative_path: String,
    content: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EditPlan {
    summary: String,
    edits: Vec<FileEdit>,
}

#[derive(Clone, Debug)]
struct RunRecord {
    summary: RunSummary,
    verifier: VerificationSpec,
    plan_hash: Option<ObjectHash>,
    edits_applied: bool,
}

pub struct RuntimeService<P> {
    provider: ProviderGateway<P>,
}

impl<P> RuntimeService<P>
where
    P: ProviderAdapter,
{
    #[must_use]
    pub const fn new(provider: ProviderGateway<P>) -> Self {
        Self { provider }
    }

    pub fn start(
        &self,
        request: RunRequest,
        cancellation: &dyn CancellationSignal,
        interrupt_after: Option<InterruptAfter>,
    ) -> Result<RunSummary, RuntimeError> {
        validate_request(&request)?;
        let snapshot =
            WorkspaceSnapshot::capture(&request.workspace_root, &SnapshotPolicy::default())
                .map_err(|error| RuntimeError::lower("workspace snapshot", error))?;
        let project_id = snapshot.identity.repo_id.clone();
        let run_id = Ulid::generate().to_string();
        let project_root = project_runtime_root(&request.state_home, &project_id);
        let worktree_path = project_root.join("worktrees").join(&run_id);
        fs::create_dir_all(worktree_path.parent().expect("run worktree has parent"))?;
        snapshot
            .materialize_owned_worktree(&worktree_path)
            .map_err(|error| RuntimeError::lower("worktree materialization", error))?;

        let mut store = open_store(&project_root)?;
        let created = json!({
            "goal": request.goal,
            "repo_id": project_id,
            "worktree_path": worktree_path,
            "verification": verification_to_json(&request.verification),
        });
        append_json(
            &mut store,
            &snapshot.identity.repo_id,
            &run_id,
            "run.created",
            created,
        )?;

        let mut record = RunRecord {
            summary: RunSummary {
                run_id: run_id.clone(),
                project_id: snapshot.identity.repo_id.clone(),
                state: RunState::Pending,
                goal: request.goal,
                worktree_path,
                provider_attempts: 0,
                verification_success: None,
                accepted: false,
                interrupted: false,
            },
            verifier: request.verification,
            plan_hash: None,
            edits_applied: false,
        };
        transition_state(&mut store, &mut record, RunState::Executing)?;
        self.obtain_plan(&mut store, &mut record, cancellation)?;

        if interrupt_after == Some(InterruptAfter::ProviderResponse) {
            append_json(
                &mut store,
                &record.summary.project_id,
                &run_id,
                "run.interrupted",
                json!({"after":"provider_response"}),
            )?;
            record.summary.interrupted = true;
            return Ok(record.summary);
        }

        continue_run(&mut store, &mut record, interrupt_after)
    }

    pub fn resume(
        &self,
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        run_id: &str,
        cancellation: &dyn CancellationSignal,
    ) -> Result<RunSummary, RuntimeError> {
        if run_id.trim().is_empty() {
            return Err(RuntimeError::InvalidRequest("run_id must not be empty"));
        }
        let workspace = WorkspaceIdentity::inspect(workspace_root.as_ref())
            .map_err(|error| RuntimeError::lower("workspace identity", error))?;
        let project_root = project_runtime_root(state_home.as_ref(), &workspace.repo_id);
        let mut store = open_store(&project_root)?;
        let mut record = load_run(&store, &workspace.repo_id, run_id)?;
        if record.summary.project_id != workspace.repo_id {
            return Err(RuntimeError::Integrity(
                "run project identity does not match current repository".to_owned(),
            ));
        }
        if record.summary.state.is_terminal() {
            return Ok(record.summary);
        }
        if !record.summary.worktree_path.is_dir() {
            return Err(RuntimeError::RecoveryRequired(format!(
                "owned worktree is missing for run {run_id}: {}",
                record.summary.worktree_path.display()
            )));
        }
        let previous_state = record.summary.state;
        let resume_target = match previous_state {
            RunState::Executing | RunState::Recovering => RunState::Executing,
            RunState::Verifying => RunState::Verifying,
            unsupported => {
                return Err(RuntimeError::RecoveryRequired(format!(
                    "run {run_id} cannot be resumed safely from {unsupported:?} by runtime 0.1"
                )));
            }
        };
        if previous_state != RunState::Recovering {
            transition_state(&mut store, &mut record, RunState::Recovering)?;
        }
        append_json(
            &mut store,
            &record.summary.project_id,
            run_id,
            "run.resumed",
            json!({
                "previous_state": run_state_name(previous_state),
                "resume_target": run_state_name(resume_target),
            }),
        )?;
        record.summary.interrupted = false;
        transition_state(&mut store, &mut record, resume_target)?;
        if record.plan_hash.is_none() {
            self.obtain_plan(&mut store, &mut record, cancellation)?;
        }
        continue_run(&mut store, &mut record, None)
    }

    fn obtain_plan(
        &self,
        store: &mut DurableState,
        record: &mut RunRecord,
        cancellation: &dyn CancellationSignal,
    ) -> Result<(), RuntimeError> {
        let attempt_id = format!("{}-provider", record.summary.run_id);
        let request = ProviderRequest {
            run_id: record.summary.run_id.clone(),
            attempt_id,
            instructions: provider_instructions(),
            input: format!(
                "Goal:\n{}\n\nOwned workspace: {}",
                record.summary.goal,
                record.summary.worktree_path.display()
            ),
            response_schema: Some(edit_plan_schema()),
        };
        let gateway = self
            .provider
            .complete(&request, cancellation)
            .map_err(|error| RuntimeError::lower("provider gateway", error))?;
        if gateway.response.output_text.len() > MAX_PLAN_BYTES {
            return Err(RuntimeError::InvalidPlan(format!(
                "provider response exceeds {MAX_PLAN_BYTES} bytes"
            )));
        }
        parse_edit_plan(&gateway.response.output_text)?;
        let metadata = ObjectMetadata {
            sensitivity: Sensitivity::Internal,
            retention_class: "run-provider-plan".to_owned(),
            expires_at: None,
            pinned: true,
        };
        let hash = store.put_object(
            &record.summary.project_id,
            gateway.response.output_text.as_bytes(),
            &metadata,
        )?;
        let mut artifact_event = NewEvent::new(&record.summary.project_id, "provider.response");
        artifact_event.run_id = Some(record.summary.run_id.clone());
        artifact_event.payload = EventPayload::Artifact(hash.clone());
        store.append_event(artifact_event)?;
        append_json(
            store,
            &record.summary.project_id,
            &record.summary.run_id,
            "provider.completed",
            json!({
                "provider_id": gateway.response.provider_id,
                "model": gateway.response.model,
                "attempts": gateway.attempts,
                "production_ready": self.provider.descriptor().production_ready,
            }),
        )?;
        record.plan_hash = Some(hash);
        record.summary.provider_attempts = gateway.attempts;
        Ok(())
    }
}

fn continue_run(
    store: &mut DurableState,
    record: &mut RunRecord,
    interrupt_after: Option<InterruptAfter>,
) -> Result<RunSummary, RuntimeError> {
    if !record.edits_applied {
        let hash = record.plan_hash.as_ref().ok_or_else(|| {
            RuntimeError::Integrity("run has no provider plan artifact".to_owned())
        })?;
        let bytes = store.read_object(&record.summary.project_id, hash)?;
        let plan_text = String::from_utf8(bytes)
            .map_err(|_| RuntimeError::InvalidPlan("provider plan is not UTF-8".to_owned()))?;
        let plan = parse_edit_plan(&plan_text)?;
        apply_edits(&record.summary.worktree_path, &plan.edits)?;
        append_json(
            store,
            &record.summary.project_id,
            &record.summary.run_id,
            "workspace.edits_applied",
            json!({"count":plan.edits.len(),"summary":plan.summary}),
        )?;
        record.edits_applied = true;
    }

    if interrupt_after == Some(InterruptAfter::EditsApplied) {
        append_json(
            store,
            &record.summary.project_id,
            &record.summary.run_id,
            "run.interrupted",
            json!({"after":"edits_applied"}),
        )?;
        record.summary.interrupted = true;
        return Ok(record.summary.clone());
    }

    if record.summary.state == RunState::Executing {
        transition_state(store, record, RunState::Verifying)?;
    }
    if record.summary.state != RunState::Verifying {
        return Err(RuntimeError::Integrity(format!(
            "cannot verify run from {:?}",
            record.summary.state
        )));
    }

    let expected_ok = verify_expected_files(
        &record.summary.worktree_path,
        &record.verifier.expected_files,
    )?;
    let environment = EnvironmentFingerprint::discover(&record.summary.worktree_path)
        .map_err(|error| RuntimeError::lower("environment fingerprint", error))?;
    let policy = ExecutionPolicy::trusted_workspace(
        &record.summary.worktree_path,
        VERIFY_TIMEOUT,
        VERIFY_CAPTURE_BYTES,
    )
    .map_err(|error| RuntimeError::lower("verification policy", error))?;
    let command = CommandSpec::new(
        &record.verifier.command.executable,
        &record.summary.worktree_path,
        SideEffectClass::ProcessExecution,
    )
    .args(record.verifier.command.args.iter());
    let result = LocalProcessExecutor
        .execute(&policy, command)
        .map_err(|error| RuntimeError::lower("verification execution", error))?;
    let evidence =
        CommandExecutionEvidence::bind(&record.summary.project_id, &environment, &result)
            .map_err(|error| RuntimeError::lower("verification evidence", error))?;
    let accepted = expected_ok && result.success;
    append_json(
        store,
        &record.summary.project_id,
        &record.summary.run_id,
        "verification.completed",
        json!({
            "success": accepted,
            "command_success": result.success,
            "expected_files_match": expected_ok,
            "evidence_digest": evidence.evidence_digest,
            "environment_digest": evidence.environment_digest,
            "stdout_sha256": evidence.stdout_sha256,
            "stderr_sha256": evidence.stderr_sha256,
            "security_profile": "direct_host_process",
        }),
    )?;
    record.summary.verification_success = Some(accepted);
    if accepted {
        append_json(
            store,
            &record.summary.project_id,
            &record.summary.run_id,
            "task.accepted",
            json!({"evidence_digest":evidence.evidence_digest}),
        )?;
        record.summary.accepted = true;
        transition_state(store, record, RunState::Completed)?;
    } else {
        transition_state(store, record, RunState::Failed)?;
    }
    store.verify_project_integrity(&record.summary.project_id)?;
    Ok(record.summary.clone())
}

pub fn list_runs(
    workspace_root: impl AsRef<Path>,
    state_home: impl AsRef<Path>,
) -> Result<Vec<RunSummary>, RuntimeError> {
    let workspace = WorkspaceIdentity::inspect(workspace_root.as_ref())
        .map_err(|error| RuntimeError::lower("workspace identity", error))?;
    let project_root = project_runtime_root(state_home.as_ref(), &workspace.repo_id);
    if !project_root.join("durable").join(".aer").exists() {
        return Ok(Vec::new());
    }
    let store = open_store(&project_root)?;
    let events = store.events(&workspace.repo_id)?;
    let run_ids = events
        .iter()
        .filter_map(|event| event.run_id.clone())
        .collect::<BTreeSet<_>>();
    run_ids
        .into_iter()
        .map(|run_id| load_run(&store, &workspace.repo_id, &run_id).map(|record| record.summary))
        .collect()
}

#[must_use]
pub fn default_state_home() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EVERYTHING_STATE_HOME") {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("everything"))
    }
    #[cfg(not(windows))]
    {
        if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
            return Some(PathBuf::from(path).join("everything"));
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".local").join("state").join("everything"))
    }
}

fn open_store(project_root: &Path) -> Result<DurableState, RuntimeError> {
    fs::create_dir_all(project_root)?;
    DurableState::open(project_root.join("durable")).map_err(RuntimeError::from)
}

fn project_runtime_root(state_home: &Path, project_id: &str) -> PathBuf {
    let digest = Sha256::digest(project_id.as_bytes());
    state_home
        .join("projects")
        .join(lowercase_hex(digest.as_ref()))
}

fn transition_state(
    store: &mut DurableState,
    record: &mut RunRecord,
    next: RunState,
) -> Result<(), RuntimeError> {
    let validated = record
        .summary
        .state
        .transition(next)
        .map_err(|error| RuntimeError::lower("run state transition", error))?;
    append_json(
        store,
        &record.summary.project_id,
        &record.summary.run_id,
        "run.state_changed",
        json!({"state":run_state_name(validated)}),
    )?;
    record.summary.state = validated;
    Ok(())
}

fn append_json(
    store: &mut DurableState,
    project_id: &str,
    run_id: &str,
    event_type: &str,
    payload: Value,
) -> Result<StoredEvent, RuntimeError> {
    let mut event = NewEvent::new(project_id, event_type);
    event.run_id = Some(run_id.to_owned());
    event.payload = EventPayload::Inline(payload);
    store.append_event(event).map_err(RuntimeError::from)
}

fn load_run(
    store: &DurableState,
    project_id: &str,
    run_id: &str,
) -> Result<RunRecord, RuntimeError> {
    let mut goal = None;
    let mut worktree = None;
    let mut verifier = None;
    let mut state = RunState::Pending;
    let mut provider_attempts = 0_u32;
    let mut plan_hash = None;
    let mut edits_applied = false;
    let mut verification_success = None;
    let mut accepted = false;
    let mut interrupted = false;

    for event in store
        .events(project_id)?
        .into_iter()
        .filter(|event| event.run_id.as_deref() == Some(run_id))
    {
        match event.event_type.as_str() {
            "run.created" => {
                let payload = inline_payload(&event)?;
                goal = payload
                    .get("goal")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                worktree = payload
                    .get("worktree_path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
                verifier = payload
                    .get("verification")
                    .map(verification_from_json)
                    .transpose()?;
            }
            "run.state_changed" => {
                let payload = inline_payload(&event)?;
                state = parse_run_state(payload.get("state").and_then(Value::as_str).ok_or_else(
                    || RuntimeError::Integrity("run state event missing state".to_owned()),
                )?)?;
            }
            "provider.response" => plan_hash = event.payload_artifact_hash,
            "provider.completed" => {
                let payload = inline_payload(&event)?;
                provider_attempts = payload
                    .get("attempts")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or(0);
            }
            "workspace.edits_applied" => edits_applied = true,
            "verification.completed" => {
                let payload = inline_payload(&event)?;
                verification_success = payload.get("success").and_then(Value::as_bool);
            }
            "task.accepted" => accepted = true,
            "run.interrupted" => interrupted = true,
            "run.resumed" => interrupted = false,
            _ => {}
        }
    }

    Ok(RunRecord {
        summary: RunSummary {
            run_id: run_id.to_owned(),
            project_id: project_id.to_owned(),
            state,
            goal: goal.ok_or_else(|| RuntimeError::UnknownRun(run_id.to_owned()))?,
            worktree_path: worktree
                .ok_or_else(|| RuntimeError::Integrity("run missing worktree path".to_owned()))?,
            provider_attempts,
            verification_success,
            accepted,
            interrupted,
        },
        verifier: verifier
            .ok_or_else(|| RuntimeError::Integrity("run missing verification spec".to_owned()))?,
        plan_hash,
        edits_applied,
    })
}

fn inline_payload(event: &StoredEvent) -> Result<Value, RuntimeError> {
    let payload = event.payload_json.as_deref().ok_or_else(|| {
        RuntimeError::Integrity(format!("{} requires inline payload", event.event_type))
    })?;
    serde_json::from_str(payload).map_err(RuntimeError::from)
}

fn verification_to_json(spec: &VerificationSpec) -> Value {
    json!({
        "command": {
            "executable": spec.command.executable,
            "args": spec.command.args,
        },
        "expected_files": spec.expected_files.iter().map(|file| json!({
            "path": file.relative_path,
            "sha256": file.sha256,
        })).collect::<Vec<_>>()
    })
}

fn verification_from_json(value: &Value) -> Result<VerificationSpec, RuntimeError> {
    let command = value
        .get("command")
        .and_then(Value::as_object)
        .ok_or_else(|| RuntimeError::Integrity("verification command missing".to_owned()))?;
    let executable = command
        .get("executable")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::Integrity("verification executable missing".to_owned()))?;
    let args = command
        .get("args")
        .and_then(Value::as_array)
        .ok_or_else(|| RuntimeError::Integrity("verification args missing".to_owned()))?
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                RuntimeError::Integrity("verification arg must be string".to_owned())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected_files = value
        .get("expected_files")
        .and_then(Value::as_array)
        .ok_or_else(|| RuntimeError::Integrity("expected_files missing".to_owned()))?
        .iter()
        .map(|entry| {
            Ok(ExpectedFile {
                relative_path: entry
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RuntimeError::Integrity("expected path missing".to_owned()))?
                    .to_owned(),
                sha256: entry
                    .get("sha256")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RuntimeError::Integrity("expected hash missing".to_owned()))?
                    .to_owned(),
            })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    Ok(VerificationSpec {
        command: VerificationCommand {
            executable: executable.to_owned(),
            args,
        },
        expected_files,
    })
}

fn provider_instructions() -> String {
    "Return only JSON matching the supplied schema. Propose bounded file-content replacements only. Do not return shell commands, credentials, or paths outside the owned workspace.".to_owned()
}

fn edit_plan_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["summary","edits"],
        "properties":{
            "summary":{"type":"string"},
            "edits":{
                "type":"array",
                "maxItems":MAX_EDITS,
                "items":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["path","content"],
                    "properties":{
                        "path":{"type":"string"},
                        "content":{"type":"string"}
                    }
                }
            }
        }
    })
}

fn parse_edit_plan(text: &str) -> Result<EditPlan, RuntimeError> {
    if text.len() > MAX_PLAN_BYTES {
        return Err(RuntimeError::InvalidPlan(
            "plan exceeds byte budget".to_owned(),
        ));
    }
    let value: Value = serde_json::from_str(text)?;
    let object = value
        .as_object()
        .ok_or_else(|| RuntimeError::InvalidPlan("plan must be an object".to_owned()))?;
    let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if keys != BTreeSet::from(["edits", "summary"]) {
        return Err(RuntimeError::InvalidPlan(
            "plan contains missing or unknown top-level fields".to_owned(),
        ));
    }
    let summary = object
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| RuntimeError::InvalidPlan("summary must be a string".to_owned()))?
        .to_owned();
    let edits = object
        .get("edits")
        .and_then(Value::as_array)
        .ok_or_else(|| RuntimeError::InvalidPlan("edits must be an array".to_owned()))?;
    if edits.is_empty() || edits.len() > MAX_EDITS {
        return Err(RuntimeError::InvalidPlan(format!(
            "edit count must be between 1 and {MAX_EDITS}"
        )));
    }
    let mut parsed = Vec::with_capacity(edits.len());
    let mut total = 0_usize;
    for edit in edits {
        let object = edit
            .as_object()
            .ok_or_else(|| RuntimeError::InvalidPlan("edit must be an object".to_owned()))?;
        let keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
        if keys != BTreeSet::from(["content", "path"]) {
            return Err(RuntimeError::InvalidPlan(
                "edit contains missing or unknown fields".to_owned(),
            ));
        }
        let relative_path = object
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| RuntimeError::InvalidPlan("edit path must be string".to_owned()))?
            .to_owned();
        validate_relative_path(&relative_path)?;
        let content = object
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| RuntimeError::InvalidPlan("edit content must be string".to_owned()))?
            .as_bytes()
            .to_vec();
        if content.len() > MAX_EDIT_BYTES {
            return Err(RuntimeError::InvalidPlan(format!(
                "edit {relative_path} exceeds {MAX_EDIT_BYTES} bytes"
            )));
        }
        total = total
            .checked_add(content.len())
            .ok_or_else(|| RuntimeError::InvalidPlan("edit byte count overflow".to_owned()))?;
        if total > MAX_PLAN_BYTES {
            return Err(RuntimeError::InvalidPlan(
                "edit payload exceeds total budget".to_owned(),
            ));
        }
        parsed.push(FileEdit {
            relative_path,
            content,
        });
    }
    Ok(EditPlan {
        summary,
        edits: parsed,
    })
}

fn apply_edits(worktree_root: &Path, edits: &[FileEdit]) -> Result<(), RuntimeError> {
    let canonical_root = worktree_root.canonicalize()?;
    for edit in edits {
        let relative = Path::new(&edit.relative_path);
        let target = canonical_root.join(relative);
        let parent = target.parent().ok_or_else(|| {
            RuntimeError::InvalidPlan(format!("edit path has no parent: {}", edit.relative_path))
        })?;
        let canonical_parent = parent.canonicalize().map_err(|_| {
            RuntimeError::InvalidPlan(format!(
                "edit parent must already exist and stay inside worktree: {}",
                edit.relative_path
            ))
        })?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(RuntimeError::InvalidPlan(format!(
                "edit escapes worktree: {}",
                edit.relative_path
            )));
        }
        if target.exists() && fs::symlink_metadata(&target)?.file_type().is_symlink() {
            return Err(RuntimeError::InvalidPlan(format!(
                "edit refuses symlink target: {}",
                edit.relative_path
            )));
        }
        fs::write(&target, &edit.content)?;
    }
    Ok(())
}

fn verify_expected_files(root: &Path, expected: &[ExpectedFile]) -> Result<bool, RuntimeError> {
    for file in expected {
        validate_relative_path(&file.relative_path)?;
        let target = root.join(&file.relative_path);
        let bytes = match fs::read(target) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if lowercase_hex(Sha256::digest(&bytes).as_ref()) != file.sha256 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_relative_path(value: &str) -> Result<(), RuntimeError> {
    if value.trim().is_empty() {
        return Err(RuntimeError::InvalidPlan("edit path is empty".to_owned()));
    }
    if value.contains('\\') || value.contains(':') || value.contains('\0') {
        return Err(RuntimeError::InvalidPlan(format!(
            "edit path must use portable forward-slash relative syntax: {value}"
        )));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RuntimeError::InvalidPlan(format!(
            "edit path must be a clean relative path: {value}"
        )));
    }
    for segment in value.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(RuntimeError::InvalidPlan(format!(
                "edit path contains an invalid component: {value}"
            )));
        }
        if segment.eq_ignore_ascii_case(".git") || segment.eq_ignore_ascii_case(".aer") {
            return Err(RuntimeError::InvalidPlan(format!(
                "edit path targets protected control-plane state: {value}"
            )));
        }
        if segment.chars().any(char::is_control) {
            return Err(RuntimeError::InvalidPlan(format!(
                "edit path contains control characters: {value}"
            )));
        }
    }
    Ok(())
}

fn validate_request(request: &RunRequest) -> Result<(), RuntimeError> {
    if request.goal.trim().is_empty() {
        return Err(RuntimeError::InvalidRequest("goal must not be empty"));
    }
    if request.verification.command.executable.trim().is_empty() {
        return Err(RuntimeError::InvalidRequest(
            "verification executable must not be empty",
        ));
    }
    if request.state_home.as_os_str().is_empty() {
        return Err(RuntimeError::InvalidRequest("state_home must not be empty"));
    }
    Ok(())
}

fn parse_run_state(value: &str) -> Result<RunState, RuntimeError> {
    match value {
        "pending" => Ok(RunState::Pending),
        "interviewing" => Ok(RunState::Interviewing),
        "planning" => Ok(RunState::Planning),
        "executing" => Ok(RunState::Executing),
        "waiting_for_user" => Ok(RunState::WaitingForUser),
        "waiting_for_permission" => Ok(RunState::WaitingForPermission),
        "verifying" => Ok(RunState::Verifying),
        "recovering" => Ok(RunState::Recovering),
        "completed" => Ok(RunState::Completed),
        "failed" => Ok(RunState::Failed),
        "cancelled" => Ok(RunState::Cancelled),
        _ => Err(RuntimeError::Integrity(format!(
            "unknown durable run state: {value}"
        ))),
    }
}

const fn run_state_name(state: RunState) -> &'static str {
    match state {
        RunState::Pending => "pending",
        RunState::Interviewing => "interviewing",
        RunState::Planning => "planning",
        RunState::Executing => "executing",
        RunState::WaitingForUser => "waiting_for_user",
        RunState::WaitingForPermission => "waiting_for_permission",
        RunState::Verifying => "verifying",
        RunState::Recovering => "recovering",
        RunState::Completed => "completed",
        RunState::Failed => "failed",
        RunState::Cancelled => "cancelled",
    }
}

#[derive(Debug)]
pub enum RuntimeError {
    InvalidRequest(&'static str),
    InvalidPlan(String),
    Integrity(String),
    RecoveryRequired(String),
    UnknownRun(String),
    LowerLayer {
        context: &'static str,
        message: String,
    },
    Io(std::io::Error),
    Json(serde_json::Error),
    Storage(aer_storage::StorageError),
}

impl RuntimeError {
    fn lower(context: &'static str, error: impl fmt::Display) -> Self {
        Self::LowerLayer {
            context,
            message: error.to_string(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => {
                write!(formatter, "invalid runtime request: {message}")
            }
            Self::InvalidPlan(message) => {
                write!(formatter, "invalid provider edit plan: {message}")
            }
            Self::Integrity(message) => write!(formatter, "runtime integrity failure: {message}"),
            Self::RecoveryRequired(message) => {
                write!(formatter, "runtime recovery required: {message}")
            }
            Self::UnknownRun(run_id) => write!(formatter, "unknown run: {run_id}"),
            Self::LowerLayer { context, message } => write!(formatter, "{context}: {message}"),
            Self::Io(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
            Self::Storage(error) => error.fmt(formatter),
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::InvalidRequest(_)
            | Self::InvalidPlan(_)
            | Self::Integrity(_)
            | Self::RecoveryRequired(_)
            | Self::UnknownRun(_)
            | Self::LowerLayer { .. } => None,
        }
    }
}

impl From<std::io::Error> for RuntimeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for RuntimeError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<aer_storage::StorageError> for RuntimeError {
    fn from(error: aer_storage::StorageError) -> Self {
        Self::Storage(error)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        time::SystemTime,
    };

    use aer_provider::{NeverCancelled, ProviderGateway, ReferenceProvider, RetryPolicy};
    use aer_workspace::WorkspaceIdentity;

    use super::{
        ExpectedFile, InterruptAfter, RunRequest, RuntimeService, VerificationCommand,
        VerificationSpec, list_runs, parse_edit_plan,
    };

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "everything-runtime-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .expect("git spawn");
        assert!(status.success(), "git command failed: {args:?}");
    }

    fn fixture_repo() -> PathBuf {
        let repo = temp_dir("repo");
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        git(&repo, &["config", "user.name", "everything fixture"]);
        fs::create_dir_all(repo.join("src")).expect("src");
        fs::write(repo.join("src/value.txt"), b"wrong\n").expect("value");
        fs::write(repo.join("notes.txt"), b"user-untracked\n").expect("untracked");
        git(&repo, &["add", "src/value.txt"]);
        git(&repo, &["commit", "-q", "-m", "fixture"]);
        repo
    }

    fn service(plan: &str) -> RuntimeService<ReferenceProvider> {
        RuntimeService::new(ProviderGateway::new(
            ReferenceProvider::fixed(plan),
            RetryPolicy::new(2, 0, 0).expect("retry policy"),
        ))
    }

    #[test]
    fn start_interrupt_resume_verify_accept_preserves_user_tree() {
        let repo = fixture_repo();
        let state_home = temp_dir("state");
        let expected = b"correct\n";
        let plan = serde_json::json!({
            "summary":"repair fixture value",
            "edits":[{"path":"src/value.txt","content":"correct\n"}]
        })
        .to_string();
        let before = WorkspaceIdentity::inspect(&repo).expect("before identity");
        let request = RunRequest {
            workspace_root: repo.clone(),
            state_home: state_home.clone(),
            goal: "make src/value.txt contain the accepted value".to_owned(),
            verification: VerificationSpec {
                command: VerificationCommand {
                    executable: "git".to_owned(),
                    args: vec!["diff".to_owned(), "--check".to_owned()],
                },
                expected_files: vec![ExpectedFile::from_bytes("src/value.txt", expected)],
            },
        };
        let interrupted = service(&plan)
            .start(
                request,
                &NeverCancelled,
                Some(InterruptAfter::ProviderResponse),
            )
            .expect("start and interrupt");
        assert!(interrupted.interrupted);
        assert_eq!(
            fs::read(repo.join("src/value.txt")).expect("user value"),
            b"wrong\n"
        );

        let resumed = service("this response must not be used")
            .resume(&repo, &state_home, &interrupted.run_id, &NeverCancelled)
            .expect("resume");
        assert!(resumed.accepted);
        assert_eq!(resumed.verification_success, Some(true));
        assert!(resumed.state.is_terminal());
        assert_eq!(
            fs::read(resumed.worktree_path.join("src/value.txt")).expect("worktree value"),
            expected
        );
        let after = WorkspaceIdentity::inspect(&repo).expect("after identity");
        assert_eq!(before, after, "user working tree changed during runtime");

        let runs = list_runs(&repo, &state_home).expect("catalog");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, resumed.run_id);
        assert!(runs[0].accepted);

        fs::remove_dir_all(state_home).expect("state cleanup");
        fs::remove_dir_all(repo).expect("repo cleanup");
    }

    #[test]
    fn provider_plan_rejects_control_plane_and_nonportable_paths() {
        for relative_path in [
            ".git/config",
            "nested/.GIT/config",
            ".aer/state.db",
            "nested/.AeR/object",
            "src\\value.txt",
            "C:/escape.txt",
            "src/value.txt:stream",
            "src//value.txt",
        ] {
            let plan = serde_json::json!({
                "summary":"bad path",
                "edits":[{"path":relative_path,"content":"bad"}]
            })
            .to_string();
            assert!(
                parse_edit_plan(&plan).is_err(),
                "provider plan unexpectedly accepted {relative_path}"
            );
        }
    }

    #[test]
    fn provider_plan_cannot_escape_owned_worktree() {
        let repo = fixture_repo();
        let state_home = temp_dir("escape-state");
        let plan = r#"{"summary":"bad","edits":[{"path":"../escape.txt","content":"bad"}]}"#;
        let request = RunRequest {
            workspace_root: repo.clone(),
            state_home: state_home.clone(),
            goal: "bad fixture".to_owned(),
            verification: VerificationSpec {
                command: VerificationCommand {
                    executable: "git".to_owned(),
                    args: vec!["diff".to_owned(), "--check".to_owned()],
                },
                expected_files: Vec::new(),
            },
        };
        assert!(service(plan).start(request, &NeverCancelled, None).is_err());
        assert!(
            !repo
                .parent()
                .expect("repo parent")
                .join("escape.txt")
                .exists()
        );
        fs::remove_dir_all(state_home).expect("state cleanup");
        fs::remove_dir_all(repo).expect("repo cleanup");
    }
}
