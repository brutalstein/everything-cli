from pathlib import Path

path = Path("crates/aer-core/src/lib.rs")
text = path.read_text(encoding="utf-8")

old_resume = '''        append_json(
            &mut store,
            &record.summary.project_id,
            run_id,
            "run.resumed",
            json!({"previous_state": run_state_name(record.summary.state)}),
        )?;
        record.summary.interrupted = false;
        if record.plan_hash.is_none() {
            self.obtain_plan(&mut store, &mut record, cancellation)?;
        }
        continue_run(&mut store, &mut record, None)
'''
new_resume = '''        let previous_state = record.summary.state;
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
'''
if old_resume not in text:
    raise SystemExit("resume replacement marker not found")
text = text.replace(old_resume, new_resume, 1)

old_validator = '''fn validate_relative_path(value: &str) -> Result<(), RuntimeError> {
    if value.trim().is_empty() {
        return Err(RuntimeError::InvalidPlan("edit path is empty".to_owned()));
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
    Ok(())
}
'''
new_validator = '''fn validate_relative_path(value: &str) -> Result<(), RuntimeError> {
    if value.trim().is_empty() {
        return Err(RuntimeError::InvalidPlan("edit path is empty".to_owned()));
    }
    if value.contains('\\\\') || value.contains(':') || value.contains('\\0') {
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
'''
if old_validator not in text:
    raise SystemExit("path validator replacement marker not found")
text = text.replace(old_validator, new_validator, 1)

old_import = '''    use super::{
        ExpectedFile, InterruptAfter, RunRequest, RuntimeService, VerificationCommand,
        VerificationSpec, list_runs,
    };
'''
new_import = '''    use super::{
        ExpectedFile, InterruptAfter, RunRequest, RuntimeService, VerificationCommand,
        VerificationSpec, list_runs, parse_edit_plan,
    };
'''
if old_import not in text:
    raise SystemExit("test import marker not found")
text = text.replace(old_import, new_import, 1)

closing_test = '''    #[test]
    fn provider_plan_cannot_escape_owned_worktree() {
'''
if closing_test not in text:
    raise SystemExit("test insertion marker not found")
new_test = '''    #[test]
    fn provider_plan_rejects_control_plane_and_nonportable_paths() {
        for relative_path in [
            ".git/config",
            "nested/.GIT/config",
            ".aer/state.db",
            "nested/.AeR/object",
            "src\\\\value.txt",
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

'''
text = text.replace(closing_test, new_test + closing_test, 1)

path.write_text(text, encoding="utf-8")
