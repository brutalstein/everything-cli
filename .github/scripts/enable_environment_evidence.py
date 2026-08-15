from pathlib import Path

path = Path("crates/aer-environment/src/lib.rs")
text = path.read_text(encoding="utf-8")
marker = "use std::{\n"
insert = "pub mod capabilities;\npub mod evidence;\n\n"
if "pub mod capabilities;" in text or "pub mod evidence;" in text:
    raise SystemExit("environment evidence modules are already enabled")
if marker not in text:
    raise SystemExit("environment module insertion marker not found")
path.write_text(text.replace(marker, insert + marker, 1), encoding="utf-8")
