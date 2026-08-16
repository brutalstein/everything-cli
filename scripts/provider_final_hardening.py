from pathlib import Path

provider = Path("crates/aer-provider/src/delegated.rs")
text = provider.read_text(encoding="utf-8")
old = '''            Self::Codex => vec![
                AuthenticationMethod::OAuthPkce,
                AuthenticationMethod::DeviceAuthorization,
                AuthenticationMethod::ApiKey,
            ],
            Self::Claude | Self::Gemini => {
                vec![
                    AuthenticationMethod::OAuthPkce,
                    AuthenticationMethod::ApiKey,
                ]
            }
'''
new = '''            Self::Codex => vec![
                AuthenticationMethod::OAuthPkce,
                AuthenticationMethod::DeviceAuthorization,
            ],
            Self::Claude | Self::Gemini => vec![AuthenticationMethod::OAuthPkce],
'''
if text.count(old) != 1:
    raise SystemExit("delegated authentication descriptor anchor mismatch")
text = text.replace(old, new, 1)
text = text.replace(
    '            &["input_tokens", "inputTokens", "prompt_tokens", "promptTokens", "prompt"],\n',
    '            &["input_tokens", "inputTokens", "prompt_tokens", "promptTokens"],\n',
    1,
)
text = text.replace(
    '''            &[
                "output_tokens",
                "outputTokens",
                "completion_tokens",
                "completionTokens",
                "candidates",
            ],
''',
    '''            &[
                "output_tokens",
                "outputTokens",
                "completion_tokens",
                "completionTokens",
            ],
''',
    1,
)
provider.write_text(text, encoding="utf-8")

cli = Path("crates/aer-cli/src/provider_cli.rs")
text = cli.read_text(encoding="utf-8")
old = '''fn contains_provider_command(args: &[OsString]) -> bool {
    args.iter().skip(1).any(|arg| {
        let value = arg.to_string_lossy();
        value == "provider" || value == "providers"
    })
}
'''
new = '''fn contains_provider_command(args: &[OsString]) -> bool {
    let mut arguments = args.iter().skip(1);
    while let Some(argument) = arguments.next() {
        let value = argument.to_string_lossy();
        if value == "--workspace" {
            let _ = arguments.next();
            continue;
        }
        if value.starts_with("--workspace=") || value.starts_with('-') {
            continue;
        }
        return value == "provider" || value == "providers";
    }
    false
}
'''
if text.count(old) != 1:
    raise SystemExit("provider lazy-router anchor mismatch")
text = text.replace(old, new, 1)
old = '''    fn provider_surface_is_lazy_for_ordinary_commands() {
        assert!(!contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("status"),
        ]));
        assert!(contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("provider"),
            OsString::from("status"),
            OsString::from("codex"),
        ]));
    }
'''
new = '''    fn provider_surface_is_lazy_for_ordinary_commands() {
        assert!(!contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("status"),
        ]));
        assert!(!contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("--workspace"),
            OsString::from("provider"),
            OsString::from("status"),
        ]));
        assert!(!contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("intent"),
            OsString::from("provider"),
        ]));
        assert!(contains_provider_command(&[
            OsString::from("everything"),
            OsString::from("--workspace=repo"),
            OsString::from("provider"),
            OsString::from("status"),
            OsString::from("codex"),
        ]));
    }
'''
if text.count(old) != 1:
    raise SystemExit("provider lazy-router test anchor mismatch")
cli.write_text(text.replace(old, new, 1), encoding="utf-8")
