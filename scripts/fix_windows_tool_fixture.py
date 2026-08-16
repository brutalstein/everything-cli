from pathlib import Path

# One-shot repair trigger for the target-Windows teardown regression.
path = Path("crates/aer-core/src/tools.rs")
text = path.read_text(encoding="utf-8")

old_import = """    fs,\n        time::{SystemTime, UNIX_EPOCH},\n    };"""
new_import = """    fs,\n        sync::atomic::{AtomicU64, Ordering},\n        time::{SystemTime, UNIX_EPOCH},\n    };"""
if old_import not in text:
    raise SystemExit("test import anchor not found")
text = text.replace(old_import, new_import, 1)

old_fixture = """    fn fixture() -> std::path::PathBuf {\n        let root = std::env::temp_dir().join(format!(\n            \"aer-tools-{}\",\n            SystemTime::now()\n                .duration_since(UNIX_EPOCH)\n                .expect(\"clock\")\n                .as_nanos()\n        ));"""
new_fixture = """    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);\n\n    fn fixture() -> std::path::PathBuf {\n        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);\n        let root = std::env::temp_dir().join(format!(\n            \"aer-tools-{}-{}-{}\",\n            std::process::id(),\n            SystemTime::now()\n                .duration_since(UNIX_EPOCH)\n                .expect(\"clock\")\n                .as_nanos(),\n            sequence\n        ));"""
if old_fixture not in text:
    raise SystemExit("fixture anchor not found")
text = text.replace(old_fixture, new_fixture, 1)

cleanup = '        fs::remove_dir_all(root).expect("cleanup");'
count = text.count(cleanup)
if count != 5:
    raise SystemExit(f"expected 5 cleanup anchors, found {count}")
text = text.replace(cleanup, '        drop(broker);\n        fs::remove_dir_all(root).expect("cleanup");')

path.write_text(text, encoding="utf-8")
