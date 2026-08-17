from pathlib import Path

path = Path(__file__).with_name("apply-context-economy-v2.py")
text = path.read_text(encoding="utf-8")
old = '''mid_start = ''' + "'''" + '''        for semantic_id in &request.required_semantic_ids {\n''' + "'''" + '''\n'''
new = '''mid_start = ''' + "'''" + '''        for semantic_id in &request.required_semantic_ids {\n            let candidate = materialized\n''' + "'''" + '''\n'''
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected one broad semantic selection anchor, found {count}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
print("Context transformer semantic-selection anchor narrowed")
