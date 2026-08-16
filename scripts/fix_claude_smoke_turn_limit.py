from pathlib import Path

path = Path("crates/aer-provider/src/delegated.rs")
text = path.read_text(encoding="utf-8")
old = '''                    OsString::from("--disable-slash-commands"),\n                    OsString::from("--no-session-persistence"),\n                    OsString::from("--max-turns"),\n                    OsString::from("1"),\n'''
new = '''                    OsString::from("--disable-slash-commands"),\n                    OsString::from("--no-session-persistence"),\n'''
if old not in text:
    raise SystemExit("Claude max-turns smoke anchor not found")
text = text.replace(old, new, 1)
path.write_text(text, encoding="utf-8")
