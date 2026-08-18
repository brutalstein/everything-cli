use std::{path::PathBuf, process::Command};

#[test]
fn parity_suite_default_mode_is_context_only_and_makes_zero_provider_calls() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = manifest_dir.join("fixtures/parity/profiles/claude_sonnet_5.toml");
    let suite = manifest_dir.join("fixtures/parity/suite.toml");

    let output = Command::new(env!("CARGO_BIN_EXE_aer-parity-benchmark"))
        .args([
            "--profile",
            profile.to_str().expect("UTF-8 profile path"),
            "--suite",
            suite.to_str().expect("UTF-8 suite path"),
            "--context-budget",
            "6144",
            "--max-output-units",
            "512",
        ])
        .output()
        .expect("run zero-provider parity diagnostic");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "context-only parity diagnostic failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("dry_run=true"),
        "parity diagnostic did not report dry-run mode\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("provider_calls=0"),
        "parity diagnostic did not prove zero provider calls\nstdout:\n{stdout}"
    );
}
