from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def patch(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one anchor, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


patch(
    "crates/aer-provider/src/lib.rs",
    "pub mod delegated;\npub mod routing;\n",
    "pub mod cognitive;\npub mod context_assembly;\npub mod delegated;\npub mod routing;\n",
)
patch(
    "crates/aer-core/src/root.rs",
    "pub mod context;\npub mod engineering_invalidation;\n",
    "pub mod context;\npub mod edit_abi;\npub mod engineering_invalidation;\n",
)
patch(
    "crates/aer-core/src/root.rs",
    "pub mod spec;\npub mod tools;\n",
    "pub mod spec;\npub mod task_working_set;\npub mod tools;\n",
)
patch(
    "crates/aer-repo/src/ri2/mod.rs",
    "mod build;\nmod freshness;\n",
    "mod build;\nmod capsule;\nmod freshness;\n",
)
patch(
    "crates/aer-repo/src/ri2/mod.rs",
    "pub use invalidation::{InvalidationFrontier, repository_file_entity_id};\npub use model::*;\n",
    "pub use capsule::*;\npub use invalidation::{InvalidationFrontier, repository_file_entity_id};\npub use model::*;\n",
)
print("Stage-2 module exports applied")
