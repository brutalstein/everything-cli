# One-shot source repair; removed before Step 13 merge.
# Repair pass: coordinator derives only what its authoritative kernels expose.
from pathlib import Path

p = Path("crates/aer-core/src/parallel.rs")
s = p.read_text(encoding="utf-8")
old = "#[derive(Debug)]\npub struct ParallelRuntimeCoordinator {\n"
if s.count(old) != 1:
    raise SystemExit(f"expected one coordinator derive, found {s.count(old)}")
s = s.replace(old, "pub struct ParallelRuntimeCoordinator {\n", 1)
p.write_text(s, encoding="utf-8")
