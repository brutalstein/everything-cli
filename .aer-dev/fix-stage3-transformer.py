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
text = text.replace(old, new, 1)

codex_block = '''text = replace_once(
    text,
    ''' + "'''" + '''                RequestPlan::new(args, self.context.merged_layers(objective))\n''' + "'''" + ''',
    ''' + "'''" + '''                RequestPlan::new(\n                    args,\n                    self.context\n                        .merged_layers(self.kind, objective)\n                        .expect(\"validated delegated context assembly\"),\n                )\n''' + "'''" + ''',
    \"codex assembled user layer\",
)
'''
gemini_block = codex_block.replace("codex assembled user layer", "gemini assembled user layer")
if text.count(codex_block) != 1 or text.count(gemini_block) != 1:
    raise SystemExit("expected distinct codex/gemini merged-layer transformer blocks")
replacement = '''merged_old = "                RequestPlan::new(args, self.context.merged_layers(objective))\\n"
merged_new = ''' + "'''" + '''                RequestPlan::new(
                    args,
                    self.context
                        .merged_layers(self.kind, objective)
                        .expect("validated delegated context assembly"),
                )
''' + "'''" + '''
if text.count(merged_old) != 2:
    raise SystemExit(f"delegated merged-layer assembly: expected two transport sites, found {text.count(merged_old)}")
text = text.replace(merged_old, merged_new, 2)
'''
text = text.replace(codex_block, replacement, 1)
text = text.replace(gemini_block, "", 1)

path.write_text(text, encoding="utf-8")
print("Stage-3 transformer narrowed and merged transport sites disambiguated")
