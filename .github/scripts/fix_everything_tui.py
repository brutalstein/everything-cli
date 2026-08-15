from pathlib import Path

path = Path("crates/aer-cli/src/lib.rs")
text = path.read_text(encoding="utf-8")
replacements = [
    ("    path::{Path, PathBuf},\n", "    path::Path,\n"),
    ("    layout::{Alignment, Constraint, Direction, Layout, Rect},\n", "    layout::{Alignment, Constraint, Layout, Rect},\n"),
    ("    ratatui::run(|terminal| {\n", "    ratatui::run(|terminal| -> io::Result<()> {\n"),
]
for old, new in replacements:
    if old not in text:
        raise SystemExit(f"expected TUI repair pattern not found: {old!r}")
    text = text.replace(old, new, 1)
path.write_text(text, encoding="utf-8")
