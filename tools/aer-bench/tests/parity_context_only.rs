use std::{path::PathBuf, process::Command};

#[test]
fn parity_suite_default_mode_is_context_only_and_makes_zero_provider_calls() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("aer-bench lives under tools/ in the workspace root");

    let output = Command::new(env!("CARGO_BIN_EXE_aer-parity-benchmark"))
        .args([
            "--workspace",
            workspace.to_str().expect("UTF-8 workspace path"),
            "--task",
            "fact_dynamic_context_budget",
            "--profile",
            "aer-production",
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
        stdout.contains("AER / Claude Code parity benchmark (DRY RUN)"),
        "parity diagnostic did not report dry-run mode\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("calls      2 planned; 0 made"),
        "parity diagnostic did not report zero executed calls\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("No provider calls were made."),
        "parity diagnostic did not prove zero provider calls\nstdout:\n{stdout}"
    );
}
