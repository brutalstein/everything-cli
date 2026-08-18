from pathlib import Path

path = Path(__file__).with_name("stage3-runtime-assembly.py")
text = path.read_text(encoding="utf-8")

start_marker = "text = replace_once(\n    text,\n    '''    use super::{\n"
start = text.find(start_marker)
if start < 0:
    raise SystemExit("runtime test transformer start not found")
end_marker = 'write(path, text)\n\nprint("Stage 3 compact runtime + provider context assembly integration applied")\n'
end = text.find(end_marker, start)
if end < 0:
    raise SystemExit("runtime test transformer end not found")

replacement = r'''new_runtime_tests = r''' + "'''" + r'''#[cfg(test)]
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
    use crate::edit_abi::sha256 as edit_sha256;

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
        let base = b"wrong\n";
        let expected = b"correct\n";
        let plan = serde_json::json!({
            "summary":"repair fixture value",
            "operations":[{
                "op":"replace_range",
                "path":"src/value.txt",
                "base_file_sha256":edit_sha256(base),
                "start_line":1,
                "end_line":1,
                "expected_segment_sha256":edit_sha256(base),
                "replacement":"correct\n"
            }]
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
                "operations":[{"op":"create_file","path":relative_path,"content":"bad"}]
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
        let plan = r#"{"summary":"bad","operations":[{"op":"create_file","path":"../escape.txt","content":"bad"}]}"#;
        let request = RunRequest {
            workspace_root: repo.clone(),
            state_home: state_home.clone(),
            goal: "bad fixture".to_owned(),
            verification: VerificationSpec {
                command: VerificationCommand {
                    executable: "git".to_owned(),
                    args: vec!["diff".to_owned(), "--check".to_owned()],
                },
                expected_files: vec![ExpectedFile::from_bytes("src/value.txt", b"wrong\n")],
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
''' + "'''" + r'''
test_start = text.find("#[cfg(test)]\nmod tests {\n")
if test_start < 0:
    raise SystemExit("runtime test module boundary not found")
text = text[:test_start] + new_runtime_tests
write(path, text)

print("Stage 3 compact runtime + provider context assembly integration applied")
'''

text = text[:start] + replacement + text[end + len(end_marker):]
path.write_text(text, encoding="utf-8")
print("Stage-3 runtime tests now use one structural module replacement")
