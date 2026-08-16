mod build;
mod freshness;
mod graph_query;
mod graph_write;
mod invalidation;
mod model;
mod schema;

pub use invalidation::{InvalidationFrontier, repository_file_entity_id};
pub use model::*;

use sha2::{Digest, Sha256};

pub(crate) use crate::RepositoryIndex;
pub(crate) use build::{BuildTopology, collect_project_topology};
pub(crate) use graph_write::rebuild_snapshot_views;
pub(crate) use schema::migrate_v1_to_v2;

pub fn language_profiles() -> Vec<LanguageProfileView> {
    crate::language::profiles()
        .iter()
        .map(|profile| LanguageProfileView {
            language_id: profile.language_id.to_owned(),
            aliases: profile
                .aliases
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            extensions: profile
                .extensions
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            filenames: profile
                .filenames
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            shebangs: profile
                .shebangs
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            file_role: profile.role.as_str().to_owned(),
            grammar_adapter: profile.grammar_adapter.map(str::to_owned),
            grammar_version: profile.grammar_version.map(str::to_owned),
            extraction_query_version: profile.extraction_query_version.to_owned(),
            maximum_static_tier: if profile.has_syntax() {
                CapabilityTier::Tier1Syntax
            } else {
                CapabilityTier::Tier0Text
            },
        })
        .collect()
}

pub(crate) fn stable_id(namespace: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"aer-ri2-id-v1\0");
    digest.update(namespace.as_bytes());
    digest.update(b"\0");
    for part in parts {
        digest.update(part.as_bytes());
        digest.update(b"\0");
    }
    format!(
        "{namespace}:{}",
        crate::sha256_digest(digest.finalize().as_ref())
    )
}

pub(crate) fn file_node_id(path: &str) -> String {
    stable_id("file", &[path])
}

pub(crate) fn package_node_id(package_id: &str) -> String {
    stable_id("package", &[package_id])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_registry_exposes_tiers_not_one_supported_boolean() {
        let profiles = language_profiles();
        assert!(
            profiles
                .iter()
                .any(|profile| profile.maximum_static_tier == CapabilityTier::Tier1Syntax)
        );
        assert!(
            profiles
                .iter()
                .any(|profile| profile.maximum_static_tier == CapabilityTier::Tier0Text)
        );
    }
}
