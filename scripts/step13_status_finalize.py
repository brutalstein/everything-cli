from pathlib import Path

path = Path("STATUS.md")
text = path.read_text(encoding="utf-8")

old_header = '''**Current phase:** Phase 7 — Bounded Parallel Execution  
**Current step:** 13 / 18 — Bounded Parallel Execution  
**Repository-side state:** IMPLEMENTED — authoritative Linux/Windows CI pending  
**Step-13 branch:** `agent/step-13-bounded-parallel-execution`  
**Step-13 focused verification:** PASS on Ubuntu during implementation; permanent ResourceBench + canonical Windows gates added  
**Next step:** 14 — Architecture Health Controller — BLOCKED until Step-13 target Windows verification passes
'''
new_header = '''**Current phase:** Phase 7 — Bounded Parallel Execution  
**Current step:** 13 / 18 — Bounded Parallel Execution  
**Repository-side state:** CI VERIFIED — awaiting target Windows reproduction  
**Verified Step-13 production code HEAD:** `fb6f10bb72c3dc2b84a1625a746611fcfd658381`  
**Verified Step-13 CI:** `foundation-ci` run `31961188080` — Ubuntu PASS including permanent ResourceBench; canonical isolated Windows verifier PASS  
**Next step:** 14 — Architecture Health Controller — BLOCKED until Step-13 target Windows verification passes
'''
if text.count(old_header) != 1:
    raise SystemExit(f"status header target count={text.count(old_header)}")
text = text.replace(old_header, new_header, 1)

old_state = '**State:** IMPLEMENTED — AUTHORITATIVE REPOSITORY CI PENDING'
if text.count(old_state) != 1:
    raise SystemExit(f"Step 13 state target count={text.count(old_state)}")
text = text.replace(old_state, '**State:** REPOSITORY CI VERIFIED — TARGET WINDOWS PENDING', 1)

anchor = '''Branches are merged into the isolated integration worktree in deterministic task order. `IntegrationBarrier` refuses acceptance until all planned merges are recorded and an integration-aware verification result is bound to the exact final integration head, repository snapshot, environment fingerprint and proof-manifest identities.
'''
addition = anchor + '''
Immediately before each merge, the integration worktree re-resolves the task branch ref and requires it to equal the locally verified `head_commit`. It also re-validates base→head ancestry and recomputes the exact changed-path set from Git. A branch commit added after local verification, a fabricated base, or stale/incomplete changed-path evidence therefore fails closed before integration.
'''
if text.count(anchor) != 1:
    raise SystemExit(f"integration hardening anchor count={text.count(anchor)}")
text = text.replace(anchor, addition, 1)

anchor = '''Preemption is conservative: only lower-priority generator work explicitly marked discardable/checkpointable can be selected. External-mutating attempts and `PreemptionSafety::Never` work are excluded.
'''
addition = anchor + '''
High-risk and explicitly serial-only work create a bidirectional serialization barrier: they cannot join already-active work, and once active they block later parallel admission until their ownership is released.
'''
if text.count(anchor) != 1:
    raise SystemExit(f"serialization anchor count={text.count(anchor)}")
text = text.replace(anchor, addition, 1)

anchor = '''- branch-local PASS cannot bypass integration verification/proof;
- real dirty-user-state snapshots can fork two isolated task worktrees, merge verified branches, and leave the user's branch/tree unchanged;
'''
addition = '''- branch-local PASS cannot bypass integration verification/proof;
- post-verification branch mutation is rejected because merge-time branch head/base/changed-path evidence is re-measured;
- high-risk serialization is enforced in both admission directions;
- real dirty-user-state snapshots can fork two isolated task worktrees, merge verified branches, and leave the user's branch/tree unchanged;
'''
if text.count(anchor) != 1:
    raise SystemExit(f"ResourceBench bullet anchor count={text.count(anchor)}")
text = text.replace(anchor, addition, 1)

replacements = {
    '| Permanent Linux ResourceBench gate | PENDING | authoritative PR CI required. |':
        '| Permanent Linux ResourceBench gate | PASS | CI `31961188080`. |',
    '| Canonical isolated Windows verifier including ResourceBench | PENDING | authoritative PR CI required. |':
        '| Canonical isolated Windows verifier including ResourceBench | PASS | CI `31961188080`. |',
    '| Full workspace regression suite | PENDING | authoritative PR CI required. |':
        '| Full workspace regression suite | PASS | CI `31961188080`. |',
    '| Temporary Step-13 workflow scaffolding removed | PENDING | remove before PR authoritative code HEAD. |':
        '| Temporary Step-13 workflow scaffolding removed | PASS | final workflow tree contains only permanent `ci.yml`. |',
}
for old, new in replacements.items():
    if text.count(old) != 1:
        raise SystemExit(f"ledger target missing: {old}")
    text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8")
