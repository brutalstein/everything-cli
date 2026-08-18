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

# Replace the brittle escaped copy of validate_relative_path with a structural
# function-boundary transform. The source implementation itself is still
# checked by exact function names and the transform fails closed if either
# boundary disappears.
validate_block_start = text.find("old_validate = '''fn validate_relative_path(value: &str)")
if validate_block_start < 0:
    raise SystemExit("runtime path validation transformer start not found")
validate_block_end_marker = 'text = replace_once(text, old_validate, new_validate, "runtime shared path validation")\n'
validate_block_end = text.find(validate_block_end_marker, validate_block_start)
if validate_block_end < 0:
    raise SystemExit("runtime path validation transformer end not found")
validate_block_end += len(validate_block_end_marker)
validate_transform = '''shared_path_validation = r''' + "'''" + '''fn validate_relative_path(value: &str) -> Result<(), RuntimeError> {
    crate::edit_abi::validate_relative_path(value)
        .map_err(|error| RuntimeError::InvalidPlan(error.to_string()))
}

''' + "'''" + '''
text = replace_between(
    text,
    "fn validate_relative_path(value: &str) -> Result<(), RuntimeError> {\\n",
    "fn validate_request(request: &RunRequest) -> Result<(), RuntimeError> {\\n",
    shared_path_validation,
    "runtime shared path validation",
)
'''
text = text[:validate_block_start] + validate_transform + text[validate_block_end:]

# Provider ContextSegment identities are cryptographic source/content hashes.
# Reuse the workspace-pinned sha2 dependency rather than adding a second hashing
# implementation or a provider-specific dependency version.
insert_before = 'write(path, text)\n\n\n# ---------------------------------------------------------------------------\n# ModelContextEnvelope:'
if text.count(insert_before) != 1:
    raise SystemExit("provider write boundary not found")
dep_patch = '''write(path, text)

path = "crates/aer-provider/Cargo.toml"
text = read(path)
text = replace_once(
    text,
    "[dependencies]\\nserde_json.workspace = true\\n",
    "[dependencies]\\nserde_json.workspace = true\\nsha2.workspace = true\\n",
    "provider sha2 workspace dependency",
)
write(path, text)


# ---------------------------------------------------------------------------
# ModelContextEnvelope:'''
text = text.replace(insert_before, dep_patch, 1)

path.write_text(text, encoding="utf-8")
print("Stage-3 transformer uses structural runtime/provider boundaries and workspace SHA-256")
