# One-shot provider source repair/format/lock trigger. Removed before merge.
from pathlib import Path

lib = Path("crates/aer-provider/src/lib.rs")
text = lib.read_text(encoding="utf-8")
old = "    pub fn scripted<I>(steps: I)\n    where\n"
new = "    pub fn scripted<I>(steps: I) -> Self\n    where\n"
if text.count(old) == 1:
    text = text.replace(old, new, 1)
elif text.count(new) != 1:
    raise SystemExit("reference provider scripted constructor is not in an expected state")
lib.write_text(text, encoding="utf-8")

path = Path("crates/aer-provider/src/delegated.rs")
text = path.read_text(encoding="utf-8")
replacements = [
    ('    ffi::{OsStr, OsString},\n', '    ffi::OsString,\n'),
    ('    workspace: PathBuf,\n', ''),
    ('        workspace: impl Into<PathBuf>,\n', ''),
    ('            workspace: workspace.into(),\n', ''),
    ('    timed_out: bool,\n', ''),
    ('    } else if result.timed_out {\n        ProviderFailureClass::Timeout\n    } else {\n', '    } else {\n'),
    ('        stderr: stderr.bytes,\n        timed_out,\n', '        stderr: stderr.bytes,\n'),
]
for old, new in replacements:
    if text.count(old) != 1:
        raise SystemExit(f"delegated provider hardening anchor mismatch: {old!r} count={text.count(old)}")
    text = text.replace(old, new, 1)
old = '''fn provider_error_from_delegated(error: DelegatedProviderError) -> ProviderError {
    let class = match error {
        DelegatedProviderError::TimedOut { .. } => ProviderFailureClass::Timeout,
        DelegatedProviderError::Spawn { ref error, .. }
            if error.kind() == io::ErrorKind::NotFound =>
        {
            ProviderFailureClass::InvalidRequest
        }
        _ => ProviderFailureClass::ProviderInternal,
    };
    ProviderError::new(class, error.to_string())
}
'''
new = '''fn provider_error_from_delegated(error: DelegatedProviderError) -> ProviderError {
    let class = match &error {
        DelegatedProviderError::TimedOut { .. } => ProviderFailureClass::Timeout,
        DelegatedProviderError::Spawn { error, .. }
            if error.kind() == io::ErrorKind::NotFound =>
        {
            ProviderFailureClass::InvalidRequest
        }
        _ => ProviderFailureClass::ProviderInternal,
    };
    ProviderError::new(class, error.to_string())
}
'''
if text.count(old) != 1:
    raise SystemExit("delegated provider error classification anchor mismatch")
text = text.replace(old, new, 1)
old = '''fn capture_bounded(mut reader: impl Read, limit: usize) -> io::Result<BoundedCapture> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if bytes.len() < limit {
            let remaining = limit - bytes.len();
            bytes.extend_from_slice(&buffer[..count.min(remaining)]);
        }
        if count > limit.saturating_sub(bytes.len()) || bytes.len() == limit {
            truncated = true;
        }
    }
    Ok(BoundedCapture { bytes, truncated })
}
'''
new = '''fn capture_bounded(mut reader: impl Read, limit: usize) -> io::Result<BoundedCapture> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let keep = count.min(remaining);
        if keep > 0 {
            bytes.extend_from_slice(&buffer[..keep]);
        }
        if keep < count {
            truncated = true;
        }
    }
    Ok(BoundedCapture { bytes, truncated })
}
'''
if text.count(old) != 1:
    raise SystemExit("bounded capture anchor mismatch")
text = text.replace(old, new, 1)
path.write_text(text, encoding="utf-8")

cli = Path("crates/aer-cli/src/provider_cli.rs")
text = cli.read_text(encoding="utf-8")
old = '''    let adapter = DelegatedCliProvider::new(
        provider,
        path,
        capsule.rendered.clone(),
        capsule.digest.clone(),
        model,
    );'''
new = '''    let adapter = DelegatedCliProvider::new(
        provider,
        capsule.rendered.clone(),
        capsule.digest.clone(),
        model,
    );'''
if text.count(old) != 1:
    raise SystemExit("provider CLI constructor anchor mismatch")
cli.write_text(text.replace(old, new, 1), encoding="utf-8")
