from pathlib import Path

root = Path("crates/aer-core/src/root.rs")
text = root.read_text(encoding="utf-8")
old = '''use std::{
    fs,
    path::{Path, PathBuf},
};
'''
new = '''use std::path::{Path, PathBuf};
'''
if old in text:
    text = text.replace(old, new, 1)
old = '''fn open_store(project_root: &Path) -> Result<DurableState, RuntimeError> {
    fs::create_dir_all(project_root)?;
    DurableState::open(project_root.join("durable")).map_err(RuntimeError::from)
}
'''
new = '''fn open_store(project_root: &Path) -> Result<DurableState, aer_storage::StorageError> {
    DurableState::open(project_root.join("durable"))
}
'''
if old in text:
    text = text.replace(old, new, 1)
if "Result<DurableState, aer_storage::StorageError>" not in text:
    raise SystemExit("root open_store repair marker not found")
root.write_text(text, encoding="utf-8")

spec = Path("crates/aer-core/src/spec.rs")
text = spec.read_text(encoding="utf-8")
old = '    use std::{fs, path::Path, process::Command, time::SystemTime};\n'
new = '''    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        time::SystemTime,
    };
'''
if old in text:
    text = text.replace(old, new, 1)
if "path::{Path, PathBuf}" not in text:
    raise SystemExit("spec test PathBuf repair marker not found")
spec.write_text(text, encoding="utf-8")
