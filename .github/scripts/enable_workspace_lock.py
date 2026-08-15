from pathlib import Path

path = Path("crates/aer-workspace/src/lib.rs")
text = path.read_text(encoding="utf-8")
marker = "const INSPECTION_OUTPUT_LIMIT: usize = 4 * 1024 * 1024;\n"
if "pub mod workspace_lock;" in text:
    raise SystemExit("workspace lock module is already enabled")
if marker not in text:
    raise SystemExit("workspace module insertion marker not found")
path.write_text(text.replace(marker, "pub mod workspace_lock;\n\n" + marker, 1), encoding="utf-8")
