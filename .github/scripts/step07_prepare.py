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

method_marker = '''    pub fn next_question(&self) -> Option<&Unknown> {
        self.intent.next_user_question()
    }
'''
method_replacement = '''    pub fn next_question(&self) -> Option<&Unknown> {
        self.intent.next_user_question()
    }

    #[must_use]
    pub fn semantic_checksum_clean(&self) -> bool {
        self.checksum
            .as_ref()
            .is_some_and(|checksum| checksum.severity == ChecksumSeverity::None)
    }
'''
if method_marker in text and "pub fn semantic_checksum_clean" not in text:
    text = text.replace(method_marker, method_replacement, 1)
if "pub fn semantic_checksum_clean" not in text:
    raise SystemExit("semantic checksum API marker not found")
spec.write_text(text, encoding="utf-8")

ui = Path("crates/aer-cli/src/ui.rs")
text = ui.read_text(encoding="utf-8")
old = '''if spec.checksum.as_ref().is_some_and(|checksum| checksum.severity == aer_domain::spec::ChecksumSeverity::None) { t.success } else { t.warning }'''
new = '''if spec.semantic_checksum_clean() { t.success } else { t.warning }'''
if old in text:
    text = text.replace(old, new, 1)
if "aer_domain::spec::ChecksumSeverity" in text:
    raise SystemExit("UI still depends directly on aer-domain")

old = '''Span::styled(tool.version.clone(), Style::default().fg(t.muted)),'''
new = '''Span::styled(
                tool.version.clone().unwrap_or_else(|| "unknown".to_owned()),
                Style::default().fg(t.muted),
            ),'''
if old in text:
    text = text.replace(old, new, 1)

old = '''fn card(title: &'static str, t: Theme, focused: bool) -> Block<'static> {'''
new = '''fn card<'a>(title: &'a str, t: Theme, focused: bool) -> Block<'a> {'''
if old in text:
    text = text.replace(old, new, 1)
if "fn card<'a>" not in text:
    raise SystemExit("card lifetime repair marker not found")
ui.write_text(text, encoding="utf-8")
