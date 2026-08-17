from pathlib import Path

path = Path(__file__).with_name("stage3-runtime-assembly.py")
text = path.read_text(encoding="utf-8")
old = 'text = replace_once(text, old_context, new_context, "delegated segmented context")\n'
new = '''text = replace_between(
    text,
    "#[derive(Clone, Debug, Eq, PartialEq)]\\npub struct DelegatedModelContext {\\n",
    "/// Production delegated adapter for local vendor-owned CLI sessions.\\n",
    new_context,
    "delegated segmented context",
)
'''
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected one delegated context replacement call, found {count}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
print("Stage-3 delegated context transformer narrowed")
