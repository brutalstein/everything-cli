//! Repository gate for architecture health.

use aer_health_check::{baseline_at_distance, check_repository, render, repository_root};

const USAGE: &str =
    "usage: aer-health-check [--check | --against <revision> | --against-distance <commits>]";

#[derive(Clone, Debug, Eq, PartialEq)]
enum CheckMode {
    Revision(String),
    Distance(usize),
}

fn parse_arguments<I, S>(arguments: I) -> Result<CheckMode, ()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arguments = arguments
        .into_iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => Ok(CheckMode::Revision("HEAD".to_owned())),
        [mode] if mode == "--check" => Ok(CheckMode::Revision("HEAD".to_owned())),
        [mode, revision] if mode == "--against" && !revision.is_empty() => {
            Ok(CheckMode::Revision(revision.clone()))
        }
        [mode, distance] if mode == "--against-distance" => distance
            .parse::<usize>()
            .ok()
            .filter(|distance| *distance > 0)
            .map(CheckMode::Distance)
            .ok_or(()),
        _ => Err(()),
    }
}

fn main() {
    let mode = match parse_arguments(std::env::args().skip(1)) {
        Ok(mode) => mode,
        Err(()) => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let root = repository_root();
    let (revision, distance) = match mode {
        CheckMode::Revision(revision) => (revision, None),
        CheckMode::Distance(distance) => match baseline_at_distance(&root, distance) {
            Ok(baseline) => (baseline.revision, Some(baseline.first_parent_distance)),
            Err(error) => {
                eprintln!("architecture health check failed: {error}");
                std::process::exit(2);
            }
        },
    };
    if let Some(distance) = distance {
        println!("health baseline: {revision} ({distance} first-parent commit(s) behind HEAD)");
    }

    match check_repository(&root, &revision) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_mode_requires_one_positive_integer() {
        assert_eq!(
            parse_arguments(["--against-distance", "12"]),
            Ok(CheckMode::Distance(12))
        );
        assert!(parse_arguments(["--against-distance", "0"]).is_err());
        assert!(parse_arguments(["--against-distance"]).is_err());
        assert!(parse_arguments(["--against-distance", "12", "extra"]).is_err());
    }
}
