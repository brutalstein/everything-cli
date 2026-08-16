use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use tree_sitter::{Language, Node, Parser};

use crate::{
    language,
    model::{EdgeKind, IndexPolicy, LanguageKind, RepoError, SymbolKind},
};

pub(crate) const TREE_SITTER_RUNTIME: &str = "0.26.11";

#[derive(Clone, Debug)]
pub(crate) struct ParsedArtifact {
    pub line_count: u32,
    pub token_count: u32,
    pub terms: Vec<TermArtifact>,
    pub symbols: Vec<LocalSymbol>,
    pub links: Vec<LocalLink>,
    pub parse_had_error: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct TermArtifact {
    pub term: String,
    pub tf: u32,
    pub first_line: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct LocalSymbol {
    pub local_id: String,
    pub name: String,
    pub kind: SymbolKind,
    pub start_byte: u32,
    pub end_byte: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub signature: String,
}

#[derive(Clone, Debug)]
pub(crate) struct LocalLink {
    pub source_local_id: Option<String>,
    pub kind: EdgeKind,
    pub target_name: String,
    pub line: u32,
}

#[must_use]
pub(crate) fn detect_language(path: &str) -> LanguageKind {
    let detection = language::detect(path);
    debug_assert!(!detection.profile_id.is_empty());
    let _role = detection.role;
    let _ambiguous = detection.ambiguous;
    detection.language
}

#[must_use]
pub(crate) fn parser_key(language: LanguageKind) -> String {
    language::parser_key(language, TREE_SITTER_RUNTIME)
}

pub(crate) fn parse_text(
    path: &str,
    text: &str,
    language: LanguageKind,
    policy: &IndexPolicy,
) -> Result<ParsedArtifact, RepoError> {
    let terms = tokenize_with_lines(text, policy.max_terms_per_file)?;
    let token_count = terms.iter().map(|term| term.tf).sum();
    let line_count = count_lines(text);

    let Some(language_runtime) = tree_sitter_language(language) else {
        return Ok(ParsedArtifact {
            line_count,
            token_count,
            terms,
            symbols: Vec::new(),
            links: Vec::new(),
            parse_had_error: false,
        });
    };

    let mut parser = Parser::new();
    parser
        .set_language(&language_runtime)
        .map_err(|error| RepoError::TreeSitter(error.to_string()))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| RepoError::TreeSitter("parser returned no syntax tree".to_owned()))?;
    let mut symbols = Vec::new();
    let mut links = Vec::new();
    let mut context = WalkContext {
        bytes: text.as_bytes(),
        path,
        language,
        policy,
        symbols: &mut symbols,
        links: &mut links,
    };
    walk(tree.root_node(), None, &mut context)?;

    symbols.sort_by(|left, right| {
        left.start_byte
            .cmp(&right.start_byte)
            .then_with(|| left.name.cmp(&right.name))
    });
    links.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
            .then_with(|| left.target_name.cmp(&right.target_name))
    });
    links.dedup_by(|left, right| {
        left.source_local_id == right.source_local_id
            && left.kind == right.kind
            && left.target_name == right.target_name
            && left.line == right.line
    });

    Ok(ParsedArtifact {
        line_count,
        token_count,
        terms,
        symbols,
        links,
        parse_had_error: tree.root_node().has_error(),
    })
}

fn tree_sitter_language(language: LanguageKind) -> Option<Language> {
    match language {
        LanguageKind::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        LanguageKind::Python => Some(tree_sitter_python::LANGUAGE.into()),
        LanguageKind::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
        LanguageKind::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        LanguageKind::Tsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        _ => None,
    }
}

struct WalkContext<'a> {
    bytes: &'a [u8],
    path: &'a str,
    language: LanguageKind,
    policy: &'a IndexPolicy,
    symbols: &'a mut Vec<LocalSymbol>,
    links: &'a mut Vec<LocalLink>,
}

fn walk(
    node: Node<'_>,
    parent_symbol: Option<&str>,
    context: &mut WalkContext<'_>,
) -> Result<(), RepoError> {
    if context.symbols.len() > context.policy.max_links_per_file
        || context.links.len() > context.policy.max_links_per_file
    {
        return Err(RepoError::Integrity(format!(
            "syntax structure for {} exceeds configured per-file bounds",
            context.path
        )));
    }

    let mut scope = parent_symbol.map(str::to_owned);
    if let Some(kind) = definition_kind(context.language, node.kind())
        && let Some(name_node) = definition_name_node(context.language, node)
        && let Ok(name) = name_node.utf8_text(context.bytes)
    {
        let name = name.trim();
        if !name.is_empty() {
            let start = u32::try_from(node.start_byte()).map_err(|_| {
                RepoError::Integrity(format!("syntax byte offset overflow in {}", context.path))
            })?;
            let end = u32::try_from(node.end_byte()).map_err(|_| {
                RepoError::Integrity(format!("syntax byte offset overflow in {}", context.path))
            })?;
            let start_line = u32::try_from(node.start_position().row + 1).map_err(|_| {
                RepoError::Integrity(format!("syntax line overflow in {}", context.path))
            })?;
            let end_line = u32::try_from(node.end_position().row + 1).map_err(|_| {
                RepoError::Integrity(format!("syntax line overflow in {}", context.path))
            })?;
            let local_id = local_symbol_id(name, kind, start, end);
            let signature = signature_for(node, context.bytes);
            let effective_kind =
                if is_test_symbol(context.path, context.language, node, name, context.bytes) {
                    SymbolKind::Test
                } else {
                    kind
                };
            context.symbols.push(LocalSymbol {
                local_id: local_id.clone(),
                name: name.to_owned(),
                kind: effective_kind,
                start_byte: start,
                end_byte: end,
                start_line,
                end_line,
                signature,
            });
            scope = Some(local_id);
        }
    }

    if is_import_node(context.language, node.kind())
        && context.links.len() < context.policy.max_links_per_file
        && let Ok(raw) = node.utf8_text(context.bytes)
    {
        let target = normalize_relation_target(raw);
        if !target.is_empty() {
            context.links.push(LocalLink {
                source_local_id: scope.clone(),
                kind: EdgeKind::Imports,
                target_name: target,
                line: line_of(node),
            });
        }
    }

    if is_call_node(node.kind())
        && context.links.len() < context.policy.max_links_per_file
        && let Some(target_node) = node.child_by_field_name("function")
        && let Some(target) = terminal_identifier(target_node, context.bytes)
    {
        context.links.push(LocalLink {
            source_local_id: scope.clone(),
            kind: EdgeKind::Calls,
            target_name: target,
            line: line_of(node),
        });
    }

    if is_reference_identifier(node, context.language)
        && context.links.len() < context.policy.max_links_per_file
        && let Ok(target) = node.utf8_text(context.bytes)
    {
        let target = target.trim();
        if target.len() >= 2 && target.len() <= 128 {
            context.links.push(LocalLink {
                source_local_id: scope.clone(),
                kind: EdgeKind::References,
                target_name: target.to_owned(),
                line: line_of(node),
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk(child, scope.as_deref(), context)?;
    }
    Ok(())
}

fn definition_kind(language: LanguageKind, kind: &str) -> Option<SymbolKind> {
    match language {
        LanguageKind::Rust => match kind {
            "function_item" => Some(SymbolKind::Function),
            "struct_item" => Some(SymbolKind::Struct),
            "enum_item" => Some(SymbolKind::Enum),
            "trait_item" => Some(SymbolKind::Trait),
            "mod_item" => Some(SymbolKind::Module),
            "type_item" => Some(SymbolKind::TypeAlias),
            "const_item" => Some(SymbolKind::Constant),
            "static_item" => Some(SymbolKind::Static),
            "macro_definition" => Some(SymbolKind::Macro),
            _ => None,
        },
        LanguageKind::Python => match kind {
            "function_definition" => Some(SymbolKind::Function),
            "class_definition" => Some(SymbolKind::Class),
            _ => None,
        },
        LanguageKind::JavaScript | LanguageKind::TypeScript | LanguageKind::Tsx => match kind {
            "function_declaration" | "generator_function_declaration" => Some(SymbolKind::Function),
            "method_definition" => Some(SymbolKind::Method),
            "class_declaration" => Some(SymbolKind::Class),
            "interface_declaration" => Some(SymbolKind::Interface),
            "type_alias_declaration" => Some(SymbolKind::TypeAlias),
            "enum_declaration" => Some(SymbolKind::Enum),
            _ => None,
        },
        _ => None,
    }
}

fn definition_name_node<'a>(language: LanguageKind, node: Node<'a>) -> Option<Node<'a>> {
    node.child_by_field_name("name").or_else(|| {
        if language == LanguageKind::Rust && node.kind() == "macro_definition" {
            node.named_child(0)
        } else {
            None
        }
    })
}

fn is_import_node(language: LanguageKind, kind: &str) -> bool {
    match language {
        LanguageKind::Rust => kind == "use_declaration" || kind == "extern_crate_declaration",
        LanguageKind::Python => kind == "import_statement" || kind == "import_from_statement",
        LanguageKind::JavaScript | LanguageKind::TypeScript | LanguageKind::Tsx => {
            kind == "import_statement"
        }
        _ => false,
    }
}

fn is_call_node(kind: &str) -> bool {
    kind == "call_expression"
}

fn is_reference_identifier(node: Node<'_>, language: LanguageKind) -> bool {
    let kind = node.kind();
    let is_identifier = match language {
        LanguageKind::Rust => kind == "identifier" || kind == "type_identifier",
        LanguageKind::Python => kind == "identifier",
        LanguageKind::JavaScript | LanguageKind::TypeScript | LanguageKind::Tsx => {
            kind == "identifier" || kind == "property_identifier" || kind == "type_identifier"
        }
        _ => false,
    };
    if !is_identifier {
        return false;
    }
    let Some(parent) = node.parent() else {
        return true;
    };
    if definition_kind(language, parent.kind()).is_some() {
        return parent
            .child_by_field_name("name")
            .is_none_or(|name| name.id() != node.id());
    }
    true
}

fn terminal_identifier(node: Node<'_>, bytes: &[u8]) -> Option<String> {
    if matches!(
        node.kind(),
        "identifier" | "field_identifier" | "property_identifier" | "type_identifier"
    ) {
        return node
            .utf8_text(bytes)
            .ok()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned);
    }
    let mut cursor = node.walk();
    let mut result = None;
    for child in node.named_children(&mut cursor) {
        if let Some(candidate) = terminal_identifier(child, bytes) {
            result = Some(candidate);
        }
    }
    result
}

fn is_test_symbol(
    path: &str,
    language: LanguageKind,
    node: Node<'_>,
    name: &str,
    bytes: &[u8],
) -> bool {
    if is_test_path(path) || name.starts_with("test_") || name.ends_with("_test") {
        return true;
    }
    if language == LanguageKind::Rust
        && let Some(parent) = node.parent()
        && parent.kind() == "attribute_item"
    {
        return parent
            .utf8_text(bytes)
            .is_ok_and(|value| value.contains("test"));
    }
    false
}

#[must_use]
pub(crate) fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("tests/")
        || lower.contains("/tests/")
        || lower.contains("/__tests__/")
        || lower.ends_with("_test.rs")
        || lower.ends_with("_test.py")
        || lower.ends_with(".test.js")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".test.tsx")
        || lower.ends_with(".spec.js")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".spec.tsx")
}

fn signature_for(node: Node<'_>, bytes: &[u8]) -> String {
    let Ok(raw) = node.utf8_text(bytes) else {
        return String::new();
    };
    let first = raw.lines().next().unwrap_or_default().trim();
    first.chars().take(512).collect()
}

fn normalize_relation_target(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(512)
        .collect()
}

fn local_symbol_id(name: &str, kind: SymbolKind, start: u32, end: u32) -> String {
    let mut digest = Sha256::new();
    digest.update(b"aer-repo-symbol-local-v1\0");
    digest.update(kind.as_str().as_bytes());
    digest.update(b"\0");
    digest.update(name.as_bytes());
    digest.update(b"\0");
    digest.update(start.to_le_bytes());
    digest.update(end.to_le_bytes());
    format!("sym-local:{}", lowercase_hex(digest.finalize().as_ref()))
}

fn line_of(node: Node<'_>) -> u32 {
    u32::try_from(node.start_position().row + 1).unwrap_or(u32::MAX)
}

fn count_lines(text: &str) -> u32 {
    if text.is_empty() {
        0
    } else {
        u32::try_from(text.bytes().filter(|byte| *byte == b'\n').count() + 1).unwrap_or(u32::MAX)
    }
}

pub(crate) fn tokenize(text: &str, limit: usize) -> Result<Vec<String>, RepoError> {
    let terms = tokenize_with_lines(text, limit)?;
    Ok(terms.into_iter().map(|term| term.term).collect())
}

fn tokenize_with_lines(text: &str, limit: usize) -> Result<Vec<TermArtifact>, RepoError> {
    let mut raw = String::new();
    let mut line = 1_u32;
    let mut counts: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut total = 0_usize;

    let mut flush = |raw: &mut String, line: u32| -> Result<(), RepoError> {
        if raw.is_empty() {
            return Ok(());
        }
        let expanded = expand_token(raw);
        raw.clear();
        for term in expanded {
            if is_stopword(&term) {
                continue;
            }
            total = total.saturating_add(1);
            if total > limit {
                return Err(RepoError::Integrity(
                    "lexical term count exceeds configured per-file bound".to_owned(),
                ));
            }
            let entry = counts.entry(term).or_insert((0, line));
            entry.0 = entry.0.saturating_add(1);
        }
        Ok(())
    };

    for ch in text.chars() {
        if ch == '\n' {
            flush(&mut raw, line)?;
            line = line.saturating_add(1);
        } else if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            if raw.len() < 256 {
                raw.push(ch);
            }
        } else {
            flush(&mut raw, line)?;
        }
    }
    flush(&mut raw, line)?;

    Ok(counts
        .into_iter()
        .map(|(term, (tf, first_line))| TermArtifact {
            term,
            tf,
            first_line,
        })
        .collect())
}

fn expand_token(raw: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let normalized = raw.to_lowercase();
    if normalized.len() >= 2 && normalized.len() <= 128 {
        terms.insert(normalized.clone());
    }
    for piece in normalized.split(['_', '-']) {
        if piece.len() >= 2 && piece.len() <= 128 {
            terms.insert(piece.to_owned());
        }
    }

    let mut current = String::new();
    let chars: Vec<char> = raw.chars().collect();
    for (index, ch) in chars.iter().copied().enumerate() {
        let boundary = index > 0
            && ch.is_uppercase()
            && chars
                .get(index - 1)
                .is_some_and(|previous| previous.is_lowercase() || previous.is_ascii_digit());
        if boundary && current.len() >= 2 {
            terms.insert(current.to_lowercase());
            current.clear();
        }
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if current.len() >= 2 {
            terms.insert(current.to_lowercase());
            current.clear();
        }
    }
    if current.len() >= 2 {
        terms.insert(current.to_lowercase());
    }
    terms
}

fn is_stopword(term: &str) -> bool {
    matches!(
        term,
        "the"
            | "and"
            | "for"
            | "with"
            | "from"
            | "this"
            | "that"
            | "into"
            | "use"
            | "pub"
            | "let"
            | "const"
            | "self"
            | "true"
            | "false"
            | "none"
            | "null"
    )
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{detect_language, parse_text, tokenize};
    use crate::model::{IndexPolicy, LanguageKind, SymbolKind};

    #[test]
    fn tokenization_expands_snake_and_camel_identifiers() {
        let terms = tokenize("verify_token verifyToken", 100).expect("tokenize");
        assert!(terms.contains(&"verify_token".to_owned()));
        assert!(terms.contains(&"verifytoken".to_owned()));
        assert!(terms.contains(&"verify".to_owned()));
        assert!(terms.contains(&"token".to_owned()));
    }

    #[test]
    fn rust_adapter_extracts_symbol_and_call() {
        let parsed = parse_text(
            "src/lib.rs",
            "fn verify_token() -> bool { true }\nfn run() { verify_token(); }",
            LanguageKind::Rust,
            &IndexPolicy::default(),
        )
        .expect("parse");
        assert!(parsed.symbols.iter().any(|symbol| {
            symbol.name == "verify_token" && symbol.kind == SymbolKind::Function
        }));
        assert!(
            parsed
                .links
                .iter()
                .any(|link| link.target_name == "verify_token")
        );
    }

    #[test]
    fn python_adapter_extracts_functions_and_calls() {
        let parsed = parse_text(
            "pkg/auth.py",
            "def verify_token(token):\n    return bool(token)\n\ndef run(token):\n    return verify_token(token)\n",
            LanguageKind::Python,
            &IndexPolicy::default(),
        )
        .expect("parse python");
        assert!(parsed.symbols.iter().any(|symbol| {
            symbol.name == "verify_token" && symbol.kind == SymbolKind::Function
        }));
        assert!(
            parsed
                .links
                .iter()
                .any(|link| link.target_name == "verify_token")
        );
    }

    #[test]
    fn javascript_adapter_extracts_functions_and_calls() {
        let parsed = parse_text(
            "web/auth.js",
            "function verifyToken(token) { return Boolean(token); }\nfunction run() { return verifyToken('x'); }\n",
            LanguageKind::JavaScript,
            &IndexPolicy::default(),
        )
        .expect("parse javascript");
        assert!(
            parsed.symbols.iter().any(|symbol| {
                symbol.name == "verifyToken" && symbol.kind == SymbolKind::Function
            })
        );
        assert!(
            parsed
                .links
                .iter()
                .any(|link| link.target_name == "verifyToken")
        );
    }

    #[test]
    fn typescript_adapter_extracts_interface_and_function() {
        let parsed = parse_text(
            "web/auth.ts",
            "interface Token { value: string }\nfunction verifyToken(token: Token): boolean { return Boolean(token.value); }\n",
            LanguageKind::TypeScript,
            &IndexPolicy::default(),
        )
        .expect("parse typescript");
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| { symbol.name == "Token" && symbol.kind == SymbolKind::Interface })
        );
        assert!(
            parsed.symbols.iter().any(|symbol| {
                symbol.name == "verifyToken" && symbol.kind == SymbolKind::Function
            })
        );
    }

    #[test]
    fn unsupported_language_uses_lexical_fallback_without_fake_symbols() {
        let parsed = parse_text(
            "README.md",
            "expired token verification behavior",
            LanguageKind::Markdown,
            &IndexPolicy::default(),
        )
        .expect("parse fallback");
        assert!(parsed.symbols.is_empty());
        assert!(parsed.links.is_empty());
        assert!(parsed.terms.iter().any(|term| term.term == "verification"));
    }

    #[test]
    fn language_detection_is_extension_driven_and_deterministic() {
        assert_eq!(detect_language("src/main.rs"), LanguageKind::Rust);
        assert_eq!(detect_language("pkg/app.py"), LanguageKind::Python);
        assert_eq!(detect_language("web/app.tsx"), LanguageKind::Tsx);
        assert_eq!(detect_language("README.md"), LanguageKind::Markdown);
    }
}
