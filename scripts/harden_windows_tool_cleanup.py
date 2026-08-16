from pathlib import Path

# One-shot hardening trigger for transient target-Windows temp-directory locks.
path = Path("crates/aer-core/src/tools.rs")
text = path.read_text(encoding="utf-8")

old_import = """        fs,\n        sync::atomic::{AtomicU64, Ordering},\n        time::{SystemTime, UNIX_EPOCH},\n    };"""
new_import = """        fs,\n        sync::atomic::{AtomicU64, Ordering},\n        thread,\n        time::{Duration, SystemTime, UNIX_EPOCH},\n    };"""
if old_import not in text:
    raise SystemExit("import anchor not found")
text = text.replace(old_import, new_import, 1)

anchor = """        fs::write(root.join(\"src/lib.rs\"), \"one\\ntwo\\nthree\\nfour\\n\").expect(\"fixture file\");\n        root\n    }\n\n    #[test]"""
replacement = """        fs::write(root.join(\"src/lib.rs\"), \"one\\ntwo\\nthree\\nfour\\n\").expect(\"fixture file\");\n        root\n    }\n\n    fn cleanup_fixture(root: std::path::PathBuf, broker: ToolBroker) {\n        drop(broker);\n        let mut last_error = None;\n        for attempt in 0..10 {\n            match fs::remove_dir_all(&root) {\n                Ok(()) => return,\n                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,\n                Err(error) => {\n                    last_error = Some(error);\n                    if attempt < 9 {\n                        thread::sleep(Duration::from_millis(25));\n                    }\n                }\n            }\n        }\n        panic!(\"cleanup: {}\", last_error.expect(\"cleanup error\"));\n    }\n\n    #[test]"""
if anchor not in text:
    raise SystemExit("cleanup helper anchor not found")
text = text.replace(anchor, replacement, 1)

old_cleanup = '        drop(broker);\n        fs::remove_dir_all(root).expect("cleanup");'
count = text.count(old_cleanup)
if count != 5:
    raise SystemExit(f"expected 5 cleanup blocks, found {count}")
text = text.replace(old_cleanup, '        cleanup_fixture(root, broker);')

path.write_text(text, encoding="utf-8")
