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

request_start = text.find('text = replace_once(\n    text,\n    \'\'\'        let request = ProviderRequest {')
if request_start < 0:
    raise SystemExit("runtime request transformer block start not found")
request_end_marker = '    "runtime provider edit evidence",\n)\n'
request_end = text.find(request_end_marker, request_start)
if request_end < 0:
    raise SystemExit("runtime request transformer block end not found")
request_end += len(request_end_marker)
request_transform = '''request_replacement = r''' + "'''" + '''        let edit_evidence = compile_edit_evidence(
            &record.summary.worktree_path,
            &record.verifier.expected_files,
        )?;
        let request = ProviderRequest {
            run_id: record.summary.run_id.clone(),
            attempt_id,
            instructions: provider_instructions(),
            input: format!(
                "Goal:\\n{}\\n\\nOwned workspace: {}\\n\\n{}",
                record.summary.goal,
                record.summary.worktree_path.display(),
                edit_evidence
            ),
            response_schema: Some(compact_edit_plan_schema(MAX_EDITS)),
        };
''' + "'''" + '''
text = replace_between(
    text,
    "        let request = ProviderRequest {\\n",
    "        let gateway = self\\n",
    request_replacement,
    "runtime provider edit evidence",
)
'''
text = text[:request_start] + request_transform + text[request_end:]

old_remove = '''text = replace_between(
    text,
    ''' + "'''" + '''#[derive(Clone, Debug, Eq, PartialEq)]
struct FileEdit {
''' + "'''" + ''',
    ''' + "'''" + '''#[derive(Clone, Debug)]
struct RunRecord {
''' + "'''" + ''',
    ''' + "'''" + '''#[derive(Clone, Debug)]
struct RunRecord {
''' + "'''" + ''',
    "remove old edit structs",
)
'''
new_remove = '''text = replace_between(
    text,
    ''' + "'''" + '''#[derive(Clone, Debug, Eq, PartialEq)]
struct FileEdit {
''' + "'''" + ''',
    ''' + "'''" + '''#[derive(Clone, Debug)]
struct RunRecord {
''' + "'''" + ''',
    "",
    "remove old edit structs",
)
'''
if text.count(old_remove) != 1:
    raise SystemExit("old edit-struct removal transformer not found")
text = text.replace(old_remove, new_remove, 1)

old_helpers = "new_helpers = '''fn provider_instructions() -> String {\n"
if text.count(old_helpers) != 1:
    raise SystemExit("runtime helper string anchor not found")
text = text.replace(old_helpers, "new_helpers = r'''fn provider_instructions() -> String {\n", 1)

path.write_text(text, encoding="utf-8")
print("Stage-3 transformer preserves Rust escapes and structural end markers")
