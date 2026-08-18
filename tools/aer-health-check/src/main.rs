//! Repository gate for architecture health.

use aer_health_check::{check_repository, render, repository_root};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (mode, revision) = (arguments.next(), arguments.next());
    let revision = match (mode.as_deref(), revision.as_deref()) {
        (Some("--check") | None, _) => "HEAD",
        (Some("--against"), Some(revision)) => revision,
        _ => {
            eprintln!("usage: aer-health-check [--check | --against <revision>]");
            std::process::exit(2);
        }
    };

    match check_repository(&repository_root(), revision) {
        Ok(report) => {
            print!("{}", render(&report));
            if report.blocked() {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("architecture health check failed: {error}");
            std::process::exit(2);
        }
    }
}
