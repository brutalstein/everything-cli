# One-shot provider source repair/format trigger. Removed before merge.
from pathlib import Path

path = Path("crates/aer-provider/src/lib.rs")
text = path.read_text(encoding="utf-8")
old = "    pub fn scripted<I>(steps: I)\n    where\n"
new = "    pub fn scripted<I>(steps: I) -> Self\n    where\n"
if text.count(old) == 1:
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
elif text.count(new) != 1:
    raise SystemExit("reference provider scripted constructor is not in an expected state")
