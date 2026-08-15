from pathlib import Path

path = Path("crates/aer-workspace/src/lib.rs")
text = path.read_text(encoding="utf-8")
old = """        if let Some(rest) = without_query.strip_prefix(scheme) {
            if let Some(at) = rest.rfind('@') {
                return format!(\"{scheme}{}\", &rest[at + 1..]);
            }
        }
"""
new = """        if let Some(rest) = without_query.strip_prefix(scheme)
            && let Some(at) = rest.rfind('@')
        {
            return format!(\"{scheme}{}\", &rest[at + 1..]);
        }
"""
if old not in text:
    raise SystemExit("expected collapsible-if block not found")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
