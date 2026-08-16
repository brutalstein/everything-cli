from pathlib import Path

path = Path("crates/aer-core/src/repository.rs")
text = path.read_text(encoding="utf-8")
old = '''#[derive(Clone, Debug)]
pub struct RepositoryService {
    policy: IndexPolicy,
}

impl Default for RepositoryService {
    fn default() -> Self {
        Self {
            policy: IndexPolicy::default(),
        }
    }
}
'''
new = '''#[derive(Clone, Debug, Default)]
pub struct RepositoryService {
    policy: IndexPolicy,
}
'''
if old not in text:
    raise SystemExit("RepositoryService Default anchor missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
