use std::process::ExitCode;

use aer_doc_check::{check_repository, repository_root};

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let first = arguments.next();
    let second = arguments.next();
    match (first.as_deref(), second) {
        (Some("--check") | None, None) => {}
        _ => {
            eprintln!("usage: aer-doc-check [--check]");
            return ExitCode::from(2);
        }
    }

    match check_repository(&repository_root()) {
        Ok(report) => {
            println!("AER documentation integrity: ok");
            println!("  numbered architecture docs: {}", report.numbered_docs);
            println!("  accepted ADRs: {}", report.accepted_adrs);
            println!("  core contract schemas: {}", report.core_schemas);
            println!("  shipped examples: {}", report.examples);
            println!("  manifest entries: {}", report.manifest_entries);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("AER documentation integrity: failed");
            eprintln!("  {error}");
            ExitCode::FAILURE
        }
    }
}
