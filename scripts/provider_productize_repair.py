from pathlib import Path

path = Path("crates/aer-provider/src/lib.rs")
text = path.read_text(encoding="utf-8")
old = "    pub fn scripted<I>(steps: I)\n    where\n"
new = "    pub fn scripted<I>(steps: I) -> Self\n    where\n"
if text.count(old) != 1:
    raise SystemExit(f"expected one scripted constructor signature, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
