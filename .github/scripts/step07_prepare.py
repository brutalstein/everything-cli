from pathlib import Path

spec = Path("crates/aer-core/src/spec.rs")
text = spec.read_text(encoding="utf-8")
text = text.replace("collections::{BTreeMap, BTreeSet},", "collections::BTreeMap,")
text = text.replace("path::{Path, PathBuf},", "path::Path,")
old = '''fn source_ref_json(source: &SourceRef) -> Value {
    json!({
        "type": match source.kind {
            SourceKind::UserMessage => "user_message",
            SourceKind::ResearchClaim => "research_claim",
            SourceKind::SystemDefault => "system_default",
            SourceKind::Repository => "repository",
            SourceKind::ArchitectureDecision => "adr",
        },
        "id": source.id,
        "detail": source.detail,
    })
}
'''
new = '''fn source_ref_json(source: &SourceRef) -> Value {
    let mut value = json!({
        "type": match source.kind {
            SourceKind::UserMessage => "user_message",
            SourceKind::ResearchClaim => "research_claim",
            SourceKind::SystemDefault => "system_default",
            SourceKind::Repository => "repository",
            SourceKind::ArchitectureDecision => "adr",
        },
        "id": source.id,
    });
    if let Some(detail) = source.detail.as_deref() {
        value
            .as_object_mut()
            .expect("source ref is object")
            .insert("detail".to_owned(), json!(detail));
    }
    value
}
'''
if old not in text:
    raise SystemExit("source_ref_json marker not found")
text = text.replace(old, new, 1)
spec.write_text(text, encoding="utf-8")

research = Path("crates/aer-research/src/lib.rs")
text = research.read_text(encoding="utf-8")
old = '''        Ok(Self {
            value,
            research_id,
            question,
            findings,
            source_count: sources.len(),
        })
'''
new = '''        let source_count = sources.len();
        Ok(Self {
            value,
            research_id,
            question,
            findings,
            source_count,
        })
'''
if old not in text:
    raise SystemExit("research source_count marker not found")
research.write_text(text.replace(old, new, 1), encoding="utf-8")

windows = Path("scripts/verify-windows.ps1")
text = windows.read_text(encoding="utf-8")
needle = '''    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-provider", "-p", "aer-core", "--all-targets", "--target", $Target
    )
'''
addition = needle + '''    Invoke-Checked -FilePath $CargoPath -Arguments @(
        "test", "--locked", "-p", "aer-domain", "-p", "aer-research", "-p", "aer-core", "--all-targets", "--target", $Target
    )
'''
if needle not in text:
    raise SystemExit("Windows Step 07 gate marker not found")
windows.write_text(text.replace(needle, addition, 1), encoding="utf-8")
