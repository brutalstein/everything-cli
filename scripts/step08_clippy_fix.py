from pathlib import Path

syntax = Path("crates/aer-repo/src/syntax.rs")
text = syntax.read_text(encoding="utf-8")

old_call = '''    walk(
        tree.root_node(),
        text.as_bytes(),
        path,
        language,
        None,
        &mut symbols,
        &mut links,
        policy,
    )?;'''
new_call = '''    let mut context = WalkContext {
        bytes: text.as_bytes(),
        path,
        language,
        policy,
        symbols: &mut symbols,
        links: &mut links,
    };
    walk(tree.root_node(), None, &mut context)?;'''
if old_call not in text:
    raise SystemExit("syntax walk invocation anchor missing")
text = text.replace(old_call, new_call, 1)

start = text.index("fn walk(\n")
end = text.index("\nfn definition_kind", start)
new_walk = r'''struct WalkContext<'a> {
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
                RepoError::Integrity(format!(
                    "syntax byte offset overflow in {}",
                    context.path
                ))
            })?;
            let end = u32::try_from(node.end_byte()).map_err(|_| {
                RepoError::Integrity(format!(
                    "syntax byte offset overflow in {}",
                    context.path
                ))
            })?;
            let start_line = u32::try_from(node.start_position().row + 1).map_err(|_| {
                RepoError::Integrity(format!("syntax line overflow in {}", context.path))
            })?;
            let end_line = u32::try_from(node.end_position().row + 1).map_err(|_| {
                RepoError::Integrity(format!("syntax line overflow in {}", context.path))
            })?;
            let local_id = local_symbol_id(name, kind, start, end);
            let signature = signature_for(node, context.bytes);
            let effective_kind = if is_test_symbol(
                context.path,
                context.language,
                node,
                name,
                context.bytes,
            ) {
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
'''
text = text[:start] + new_walk + text[end:]

old_test_symbol = r'''fn is_test_symbol(
    path: &str,
    language: LanguageKind,
    node: Node<'_>,
    name: &str,
    bytes: &[u8],
) -> bool {
    if is_test_path(path) || name.starts_with("test_") || name.ends_with("_test") {
        return true;
    }
    if language == LanguageKind::Rust {
        if let Some(parent) = node.parent() {
            if parent.kind() == "attribute_item" {
                return parent
                    .utf8_text(bytes)
                    .is_ok_and(|value| value.contains("test"));
            }
        }
    }
    false
}'''
new_test_symbol = r'''fn is_test_symbol(
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
}'''
if old_test_symbol not in text:
    raise SystemExit("test symbol anchor missing")
text = text.replace(old_test_symbol, new_test_symbol, 1)

insert_anchor = '''    #[test]
    fn language_detection_is_extension_driven_and_deterministic() {'''
extra_tests = r'''    #[test]
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
        assert!(parsed.links.iter().any(|link| link.target_name == "verify_token"));
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
        assert!(parsed.symbols.iter().any(|symbol| {
            symbol.name == "verifyToken" && symbol.kind == SymbolKind::Function
        }));
        assert!(parsed.links.iter().any(|link| link.target_name == "verifyToken"));
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
        assert!(parsed.symbols.iter().any(|symbol| {
            symbol.name == "Token" && symbol.kind == SymbolKind::Interface
        }));
        assert!(parsed.symbols.iter().any(|symbol| {
            symbol.name == "verifyToken" && symbol.kind == SymbolKind::Function
        }));
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

'''
if insert_anchor not in text:
    raise SystemExit("syntax test insertion anchor missing")
text = text.replace(insert_anchor, extra_tests + insert_anchor, 1)
syntax.write_text(text, encoding="utf-8")

lib = Path("crates/aer-repo/src/lib.rs")
text = lib.read_text(encoding="utf-8")
old_recall = "        let recall = if total == 0 { 1000 } else { u16::try_from((found * 1000) / total).unwrap_or(1000) };"
new_recall = '''        let recall = found
            .saturating_mul(1000)
            .checked_div(total)
            .map_or(1000, |value| u16::try_from(value).unwrap_or(1000));'''
if old_recall not in text:
    raise SystemExit("retrieval recall anchor missing")
text = text.replace(old_recall, new_recall, 1)
lib.write_text(text, encoding="utf-8")
