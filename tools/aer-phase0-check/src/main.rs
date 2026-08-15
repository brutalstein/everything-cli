use std::process::ExitCode;

use aer_phase0_check::check_repository;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if let Some(argument) = args.next()
        && (argument != "--check" || args.next().is_some())
    {
        eprintln!("usage: aer-phase0-check [--check]");
        return ExitCode::from(2);
    }

    match check_repository(&repository_root()) {
        Ok(report) => {
            println!("AER Phase 0 executable contracts: ok");
            println!(
                "  compiled Draft 2020-12 schemas: {}",
                report.compiled_schemas
            );
            println!("  shipped examples validated: {}", report.shipped_examples);
            println!(
                "  structural negative fixtures: {}",
                report.structural_negative_fixtures
            );
            println!("  semantic fixtures: {}", report.semantic_fixtures);
            println!(
                "  compatibility fixtures: {}",
                report.compatibility_fixtures
            );
            println!(
                "  normative config YAML blocks: {}",
                report.normative_config_blocks
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("AER Phase 0 executable contracts: FAILED\n{error}");
            ExitCode::FAILURE
        }
    }
}

fn repository_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
