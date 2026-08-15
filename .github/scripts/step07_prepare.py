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
text = text.replace('const PRODUCT: &str = "everything";\n', '')
if "fn card<'a>" not in text:
    raise SystemExit("card lifetime repair marker not found")
ui.write_text(text, encoding="utf-8")

icons = Path("crates/aer-cli/src/material_icons.rs")
text = icons.read_text(encoding="utf-8")
if "pub fn sources_integrity_ok" not in text:
    marker = '''pub const ALL: [(&str, MaterialIcon); 14] = [
'''
    if marker not in text:
        raise SystemExit("material ALL marker not found")
    insert_after = '''pub const ALL: [(&str, MaterialIcon); 14] = [
    ("home", HOME),
    ("intent", INTENT),
    ("research", RESEARCH),
    ("engineering_ir", ENGINEERING_IR),
    ("workspace", WORKSPACE),
    ("environment", ENVIRONMENT),
    ("providers", PROVIDERS),
    ("activity", ACTIVITY),
    ("settings", SETTINGS),
    ("branch", BRANCH),
    ("ready", READY),
    ("attention", ATTENTION),
    ("shield", SHIELD),
    ("arrow", ARROW),
];
'''
    replacement = insert_after + '''
#[must_use]
pub fn sources_integrity_ok() -> bool {
    use sha2::{Digest, Sha256};

    ALL.iter().all(|(_, icon)| {
        if !icon.asset.starts_with(b"<svg") {
            return false;
        }
        let digest = Sha256::digest(icon.asset);
        let actual = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        actual == icon.sha256
    })
}
'''
    if insert_after not in text:
        raise SystemExit("material ALL block changed unexpectedly")
    text = text.replace(insert_after, replacement, 1)

old_test = '''    use sha2::{Digest, Sha256};

    use super::ALL;

    #[test]
    fn vendored_material_assets_match_recorded_sha256_and_are_svg() {
        for (name, icon) in ALL {
            assert!(
                icon.asset.starts_with(b"<svg"),
                "{name} vendored asset is not SVG"
            );
            let digest = Sha256::digest(icon.asset);
            let actual = digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            assert_eq!(actual, icon.sha256, "{name} source asset changed");
            assert!(!icon.compact.trim().is_empty());
            assert!(!icon.ascii.trim().is_empty());
        }
    }
'''
new_test = '''    use super::{ALL, sources_integrity_ok};

    #[test]
    fn vendored_material_assets_match_recorded_sha256_and_are_svg() {
        assert!(sources_integrity_ok());
        for (name, icon) in ALL {
            assert!(!icon.compact.trim().is_empty(), "{name} compact projection");
            assert!(!icon.ascii.trim().is_empty(), "{name} ASCII fallback");
        }
    }
'''
if old_test in text:
    text = text.replace(old_test, new_test, 1)
if "pub fn sources_integrity_ok" not in text:
    raise SystemExit("material runtime integrity function missing")
icons.write_text(text, encoding="utf-8")

theme = Path("crates/aer-cli/src/theme.rs")
text = theme.read_text(encoding="utf-8")
old = '''        let ascii = std::env::var_os("EVERYTHING_ASCII").is_some();
        let no_color = std::env::var_os("NO_COLOR").is_some();
'''
new = '''        let ascii = std::env::var_os("EVERYTHING_ASCII").is_some()
            || !material_icons::sources_integrity_ok();
        let no_color = std::env::var_os("NO_COLOR").is_some();
'''
if old in text:
    text = text.replace(old, new, 1)
if "material_icons::sources_integrity_ok" not in text:
    raise SystemExit("theme does not consume Material source integrity")
theme.write_text(text, encoding="utf-8")
