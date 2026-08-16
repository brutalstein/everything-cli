use crate::model::LanguageKind;

pub(crate) const LANGUAGE_REGISTRY_VERSION: &str = "ri2-language-registry-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileRole {
    Programming,
    Data,
    Prose,
    Configuration,
    Script,
    Unknown,
}

impl FileRole {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Programming => "programming",
            Self::Data => "data",
            Self::Prose => "prose",
            Self::Configuration => "configuration",
            Self::Script => "script",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LanguageProfile {
    pub language: LanguageKind,
    pub language_id: &'static str,
    pub aliases: &'static [&'static str],
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub shebangs: &'static [&'static str],
    pub role: FileRole,
    pub grammar_adapter: Option<&'static str>,
    pub grammar_version: Option<&'static str>,
    pub extraction_query_version: &'static str,
}

impl LanguageProfile {
    pub(crate) const fn has_syntax(self) -> bool {
        self.grammar_adapter.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LanguageDetection {
    pub language: LanguageKind,
    pub profile_id: &'static str,
    pub role: FileRole,
    pub ambiguous: bool,
}

const PROFILES: &[LanguageProfile] = &[
    LanguageProfile {
        language: LanguageKind::Rust,
        language_id: "rust",
        aliases: &["rust", "rs"],
        extensions: &["rs"],
        filenames: &[],
        shebangs: &[],
        role: FileRole::Programming,
        grammar_adapter: Some("tree-sitter-rust"),
        grammar_version: Some("0.24.2"),
        extraction_query_version: "aer-v2",
    },
    LanguageProfile {
        language: LanguageKind::Python,
        language_id: "python",
        aliases: &["python", "py"],
        extensions: &["py", "pyi"],
        filenames: &[],
        shebangs: &["python", "python3"],
        role: FileRole::Programming,
        grammar_adapter: Some("tree-sitter-python"),
        grammar_version: Some("0.25.0"),
        extraction_query_version: "aer-v2",
    },
    LanguageProfile {
        language: LanguageKind::JavaScript,
        language_id: "javascript",
        aliases: &["javascript", "js", "jsx"],
        extensions: &["js", "mjs", "cjs", "jsx"],
        filenames: &[],
        shebangs: &["node"],
        role: FileRole::Programming,
        grammar_adapter: Some("tree-sitter-javascript"),
        grammar_version: Some("0.25.0"),
        extraction_query_version: "aer-v2",
    },
    LanguageProfile {
        language: LanguageKind::TypeScript,
        language_id: "typescript",
        aliases: &["typescript", "ts"],
        extensions: &["ts", "mts", "cts"],
        filenames: &[],
        shebangs: &[],
        role: FileRole::Programming,
        grammar_adapter: Some("tree-sitter-typescript"),
        grammar_version: Some("0.23.2"),
        extraction_query_version: "aer-v2",
    },
    LanguageProfile {
        language: LanguageKind::Tsx,
        language_id: "tsx",
        aliases: &["tsx"],
        extensions: &["tsx"],
        filenames: &[],
        shebangs: &[],
        role: FileRole::Programming,
        grammar_adapter: Some("tree-sitter-tsx"),
        grammar_version: Some("0.23.2"),
        extraction_query_version: "aer-v2",
    },
    LanguageProfile {
        language: LanguageKind::Json,
        language_id: "json",
        aliases: &["json", "jsonc"],
        extensions: &["json", "jsonc"],
        filenames: &[],
        shebangs: &[],
        role: FileRole::Data,
        grammar_adapter: None,
        grammar_version: None,
        extraction_query_version: "lexical-v2",
    },
    LanguageProfile {
        language: LanguageKind::Toml,
        language_id: "toml",
        aliases: &["toml"],
        extensions: &["toml"],
        filenames: &["Cargo.lock"],
        shebangs: &[],
        role: FileRole::Configuration,
        grammar_adapter: None,
        grammar_version: None,
        extraction_query_version: "lexical-v2",
    },
    LanguageProfile {
        language: LanguageKind::Markdown,
        language_id: "markdown",
        aliases: &["markdown", "md", "mdx"],
        extensions: &["md", "mdx"],
        filenames: &[],
        shebangs: &[],
        role: FileRole::Prose,
        grammar_adapter: None,
        grammar_version: None,
        extraction_query_version: "lexical-v2",
    },
    LanguageProfile {
        language: LanguageKind::Shell,
        language_id: "shell",
        aliases: &["shell", "bash", "sh", "powershell"],
        extensions: &["sh", "bash", "zsh", "ps1"],
        filenames: &[],
        shebangs: &["sh", "bash", "zsh", "pwsh"],
        role: FileRole::Script,
        grammar_adapter: None,
        grammar_version: None,
        extraction_query_version: "lexical-v2",
    },
    LanguageProfile {
        language: LanguageKind::Yaml,
        language_id: "yaml",
        aliases: &["yaml", "yml"],
        extensions: &["yaml", "yml"],
        filenames: &[],
        shebangs: &[],
        role: FileRole::Configuration,
        grammar_adapter: None,
        grammar_version: None,
        extraction_query_version: "lexical-v2",
    },
];

pub(crate) fn profiles() -> &'static [LanguageProfile] {
    PROFILES
}

pub(crate) fn profile(language: LanguageKind) -> Option<&'static LanguageProfile> {
    PROFILES.iter().find(|profile| profile.language == language)
}

pub(crate) fn detect(path: &str) -> LanguageDetection {
    let filename = path.rsplit('/').next().unwrap_or(path);
    let exact = PROFILES
        .iter()
        .filter(|profile| {
            profile
                .filenames
                .iter()
                .any(|candidate| candidate == &filename)
        })
        .collect::<Vec<_>>();
    if exact.len() == 1 {
        return detection(exact[0], false);
    }
    if exact.len() > 1 {
        return fallback(true);
    }

    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let Some(extension) = extension else {
        return fallback(false);
    };
    let matches = PROFILES
        .iter()
        .filter(|profile| {
            profile
                .extensions
                .iter()
                .any(|candidate| *candidate == extension)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [profile] => detection(profile, false),
        [] => fallback(false),
        _ => fallback(true),
    }
}

pub(crate) fn parser_key(language: LanguageKind, runtime: &str) -> String {
    let Some(profile) = profile(language) else {
        return format!("text-lexical-v2/{LANGUAGE_REGISTRY_VERSION}");
    };
    match (profile.grammar_adapter, profile.grammar_version) {
        (Some(adapter), Some(version)) => format!(
            "{adapter}@{version}/runtime-{runtime}/{}/{LANGUAGE_REGISTRY_VERSION}",
            profile.extraction_query_version
        ),
        _ => format!(
            "text-lexical-v2/{}/{LANGUAGE_REGISTRY_VERSION}",
            profile.extraction_query_version
        ),
    }
}

fn detection(profile: &LanguageProfile, ambiguous: bool) -> LanguageDetection {
    LanguageDetection {
        language: profile.language,
        profile_id: profile.language_id,
        role: profile.role,
        ambiguous,
    }
}

fn fallback(ambiguous: bool) -> LanguageDetection {
    LanguageDetection {
        language: LanguageKind::Other,
        profile_id: "unclassified-text",
        role: FileRole::Unknown,
        ambiguous,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_detection_is_deterministic_and_unknowns_fall_back() {
        assert_eq!(detect("src/lib.rs").language, LanguageKind::Rust);
        assert_eq!(detect("Cargo.lock").language, LanguageKind::Toml);
        assert_eq!(detect("src/header.h").language, LanguageKind::Other);
        assert!(!detect("src/header.h").ambiguous);
    }

    #[test]
    fn every_native_syntax_profile_has_versioned_cache_identity() {
        for profile in profiles().iter().filter(|profile| profile.has_syntax()) {
            let key = parser_key(profile.language, "0.26.11");
            assert!(key.contains(profile.language_id));
            assert!(key.contains(LANGUAGE_REGISTRY_VERSION));
            assert!(profile.grammar_version.is_some());
        }
    }
}
