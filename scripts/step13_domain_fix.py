# One-shot source repair; removed before Step 13 merge.
from pathlib import Path

p = Path("crates/aer-domain/src/scheduling.rs")
s = p.read_text(encoding="utf-8")
old = '        let graph = TaskGraph::new(vec![a.clone(), b.clone()]).expect("graph");\n'
if s.count(old) != 1:
    raise SystemExit(f"expected one unused graph binding, found {s.count(old)}")
s = s.replace(old, "", 1)
p.write_text(s, encoding="utf-8")
