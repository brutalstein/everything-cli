# One-shot provider productization repair and manifest refresh. Removed before merge.
from pathlib import Path
import hashlib

provider = Path("crates/aer-provider/src/delegated.rs")
text = provider.read_text(encoding="utf-8")
anchor = '''    #[test]
    fn single_json_parser_supports_claude_and_gemini_shape() {
'''
extra = '''    #[test]
    fn bounded_capture_distinguishes_exact_limit_from_overflow() {
        let exact = super::capture_bounded(std::io::Cursor::new(b"abcd"), 4)
            .expect("exact capture");
        assert_eq!(exact.bytes, b"abcd");
        assert!(!exact.truncated);

        let overflow = super::capture_bounded(std::io::Cursor::new(b"abcde"), 4)
            .expect("overflow capture");
        assert_eq!(overflow.bytes, b"abcd");
        assert!(overflow.truncated);
    }

'''
if extra not in text:
    if text.count(anchor) != 1:
        raise SystemExit("provider test insertion anchor missing")
    text = text.replace(anchor, extra + anchor, 1)
provider.write_text(text, encoding="utf-8")

tools = Path("crates/aer-core/src/tools.rs")
text = tools.read_text(encoding="utf-8")
anchor = '''    #[test]
    fn tool_catalog_is_progressively_disclosed() {
'''
extra = '''    #[test]
    fn auto_mode_executes_structured_local_command_with_bounded_evidence() {
        let root = fixture();
        let broker = ToolBroker::new(&root).expect("broker");
        let permissions = PermissionController::developer_workspace(PermissionMode::Auto);
        let result = broker
            .execute(
                &permissions,
                ToolCall::ExecRun {
                    program: "git".to_owned(),
                    args: vec!["--version".to_owned()],
                    cwd: None,
                    reason: "verify structured command transport".to_owned(),
                },
            )
            .expect("exec");
        let ToolOutcome::Completed(ToolResult::Exec(exec)) = result else {
            panic!("auto mode should execute eligible structured command");
        };
        assert!(exec.success);
        assert!(!exec.timed_out);
        assert_eq!(exec.argv.first().map(String::as_str), Some("git"));
        assert!(exec.stdout_preview.contains("git version"));
        assert!(!exec.stdout_sha256.is_empty());
        fs::remove_dir_all(root).expect("cleanup");
    }

'''
if extra not in text:
    if text.count(anchor) != 1:
        raise SystemExit("tool test insertion anchor missing")
    text = text.replace(anchor, extra + anchor, 1)
tools.write_text(text, encoding="utf-8")

# Regenerate the docs inventory with real SHA-256 values. The current checker
# enforces coverage; keeping real hashes preserves the manifest's audit value.
docs = Path("docs")
manifest = docs / "MANIFEST.sha256"
entries = []
for path in sorted(docs.rglob("*")):
    if not path.is_file() or path == manifest:
        continue
    relative = path.as_posix()
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    entries.append(f"{digest}  {relative}")
manifest.write_text("\n".join(entries) + "\n", encoding="utf-8")
