# One-shot provider source repair/format/lock trigger. Removed before merge.
from pathlib import Path

path = Path("crates/aer-provider/src/delegated.rs")
text = path.read_text(encoding="utf-8")
old = '''            if item.get("type").and_then(Value::as_str) == Some("agent_message") {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    output = Some(text.to_owned());
                }
            }
'''
new = '''            if item.get("type").and_then(Value::as_str) == Some("agent_message")
                && let Some(text) = item.get("text").and_then(Value::as_str)
            {
                output = Some(text.to_owned());
            }
'''
if text.count(old) == 1:
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
elif text.count(new) != 1:
    raise SystemExit("Codex agent-message parser is not in an expected state")
