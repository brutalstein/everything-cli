//! Architecture health applied to this repository.
//!
//! `docs/18` ends with the rule that makes this tool exist: AER's own codebase
//! must be subject to the same health controller it applies to other people's.
//! An orchestration product that erodes its own runtime has disproved its thesis.
//!
//! Two checks run here, and they are deliberately different in kind.
//!
//! The **layer gate** is absolute and blocking. Crate layering is a declared
//! architectural fact, so a dependency that crosses it is wrong today, not
//! merely worse than yesterday.
//!
//! The **health delta** is relative and advisory. Local checks may measure the
//! working tree against `HEAD`; CI selects an explicit first-parent distance so
//! slow erosion cannot hide below a one-change threshold. It reports what the
//! selected interval worsened without failing the build, because a pre-existing
//! hotspot must not block unrelated work and a threshold tight enough to be
//! useful is too tight to be automatic.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use aer_exec::{
    CommandSpec, ExecutionError, ExecutionPolicy, LocalProcessExecutor, SideEffectClass,
};
use aer_health::{
    BoundaryViolation, FileHealth, Finding, HealthPolicy, HealthSnapshot, HealthVerdict, Layer,
    LayerRules, UnitSpan, compare, evaluate, scan_duplication,
};
use aer_repo::measure_source;

/// Wall-clock ceiling for the Git commands this tool runs.
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

/// Capture ceiling for Git output. A file larger than this is not measured.
const GIT_CAPTURE_BYTES: usize = 8 * 1024 * 1024;

/// The declared crate layering of this workspace.
///
/// It follows the structure in `docs/27`. The rule with teeth is the direction:
/// nothing lower may depend on something higher, so a future change that makes
/// the process substrate depend on the orchestration layer fails here rather
/// than being discovered when someone tries to reuse it.
///
/// Tools are absent on purpose. A measurement harness that reaches into several
/// layers is doing its job, and constraining it would only invite exceptions.
#[must_use]
pub fn workspace_layers() -> LayerRules {
    LayerRules::new([
        Layer::new("domain", ["crates/aer-domain"], Vec::<String>::new()),
        Layer::new("contracts", ["crates/aer-contracts"], ["domain"]),
        Layer::new(
            "application",
            ["crates/aer-core"],
            ["domain", "contracts", "infrastructure"],
        ),
        Layer::new(
            "client",
            ["crates/aer-cli"],
            ["application", "infrastructure"],
        ),
        Layer::new("infrastructure", ["crates/aer-"], ["domain", "contracts"]),
    ])
}

/// One workspace member and the sibling crates it depends on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberDependencies {
    /// Manifest directory relative to the workspace root, with forward slashes.
    pub directory: String,
    /// Directories of the workspace crates this member depends on.
    pub depends_on: Vec<String>,
}

/// Reads the workspace member list from the root manifest.
///
/// # Errors
///
/// Fails when the manifest cannot be read.
pub fn workspace_members(root: &Path) -> Result<Vec<String>, HealthCheckError> {
    let manifest = fs::read_to_string(root.join("Cargo.toml")).map_err(HealthCheckError::Io)?;
    let mut members = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with("members") {
            inside = true;
            continue;
        }
        if inside {
            if line.starts_with(']') {
                break;
            }
            if let Some(member) = line
                .strip_prefix('"')
                .and_then(|rest| rest.split('"').next())
            {
                members.push(member.to_owned());
            }
        }
    }
    Ok(members)
}

/// Reads the sibling-crate dependencies each member declares.
///
/// The manifests are repository-owned and uniform, so the path dependencies are
/// read directly rather than by adding a TOML parser for five lines of shape.
///
/// # Errors
///
/// Fails when a member manifest cannot be read.
pub fn member_dependencies(
    root: &Path,
    members: &[String],
) -> Result<Vec<MemberDependencies>, HealthCheckError> {
    let mut all = Vec::new();
    for member in members {
        let manifest = fs::read_to_string(root.join(member).join("Cargo.toml"))
            .map_err(HealthCheckError::Io)?;
        let mut depends_on = Vec::new();
        for line in manifest.lines() {
            let line = line.trim();
            let Some((name, rest)) = line.split_once(" = ") else {
                continue;
            };
            if !name.starts_with("aer-") || !rest.contains("path = ") {
                continue;
            }
            let Some(relative) = rest
                .split("path = \"")
                .nth(1)
                .and_then(|r| r.split('"').next())
            else {
                continue;
            };
            depends_on.push(normalize(&join_relative(member, relative)));
        }
        all.push(MemberDependencies {
            directory: normalize(member),
            depends_on,
        });
    }
    Ok(all)
}

/// Every declared dependency that crosses a forbidden layer boundary.
#[must_use]
pub fn boundary_violations(
    rules: &LayerRules,
    members: &[MemberDependencies],
) -> Vec<BoundaryViolation> {
    let edges = members
        .iter()
        .flat_map(|member| {
            member
                .depends_on
                .iter()
                .map(|target| (member.directory.as_str(), target.as_str()))
        })
        .collect::<Vec<_>>();
    rules.violations(edges)
}

/// Measures the working-tree health of the given source files.
///
/// Files that cannot be read are skipped rather than guessed at: a measurement
/// of a file this tool could not see would be a fabricated data point.
///
/// # Errors
///
/// Fails when a readable file cannot be measured by the language adapters.
pub fn measure_working_tree(
    root: &Path,
    paths: &[String],
) -> Result<HealthSnapshot, HealthCheckError> {
    let mut sources = Vec::new();
    for path in paths {
        if let Ok(text) = fs::read_to_string(root.join(path)) {
            sources.push((path.clone(), text));
        }
    }
    snapshot_of(&sources)
}

/// Measures the committed health of the given source files at one revision.
///
/// A path absent from the revision is a new file, and is simply not recorded:
/// the comparison then measures it against zero, which is what adding it did.
///
/// # Errors
///
/// Fails when Git cannot run or a retrieved file cannot be measured.
pub fn measure_revision(
    root: &Path,
    revision: &str,
    paths: &[String],
) -> Result<HealthSnapshot, HealthCheckError> {
    let policy = ExecutionPolicy::trusted_workspace(root, GIT_TIMEOUT, GIT_CAPTURE_BYTES)
        .map_err(HealthCheckError::Execution)?;
    let mut sources = Vec::new();
    for path in paths {
        let spec = CommandSpec::new("git", root, SideEffectClass::PureRead)
            .args(["show", &format!("{revision}:{path}")]);
        let result = LocalProcessExecutor
            .execute(&policy, spec)
            .map_err(HealthCheckError::Execution)?;
        if !result.success || result.stdout.truncated {
            continue;
        }
        let Ok(text) = String::from_utf8(result.stdout.preview) else {
            continue;
        };
        sources.push((path.clone(), text));
    }
    snapshot_of(&sources)
}

/// Measures a set of source buffers, including the duplication between them.
///
/// Duplication is scanned over the whole set rather than per file, because a
/// block is only duplicated relative to something else. Scanning only the
/// changed files is the honest scope for a delta: it detects a block copied
/// within the change, and does not claim to know what the rest of the
/// repository already contains.
fn snapshot_of(sources: &[(String, String)]) -> Result<HealthSnapshot, HealthCheckError> {
    let duplication = scan_duplication(
        sources
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str())),
    );
    let mut snapshot = HealthSnapshot::new();
    for (path, text) in sources {
        let health =
            measure(path, text)?.with_duplicated_lines(duplication.get(path).copied().unwrap_or(0));
        snapshot = snapshot.with_file(path.clone(), health);
    }
    Ok(snapshot)
}

/// A commit selected by an explicit first-parent distance from `HEAD`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaselineRevision {
    /// Full commit identity returned by Git.
    pub revision: String,
    /// Number of first-parent edges between `HEAD` and `revision`.
    pub first_parent_distance: usize,
}

/// Resolves the commit exactly `distance` first-parent edges behind `HEAD`.
///
/// This fails closed when the requested history is unavailable. Falling back
/// to a nearer commit would silently weaken the drift measurement that the
/// caller explicitly requested.
///
/// # Errors
///
/// Fails for zero/overflowing distance, unavailable history, or a Git failure.
pub fn baseline_at_distance(
    root: &Path,
    distance: usize,
) -> Result<BaselineRevision, HealthCheckError> {
    let count = distance
        .checked_add(1)
        .filter(|_| distance > 0)
        .ok_or(HealthCheckError::InvalidBaselineDistance)?;
    let policy = ExecutionPolicy::trusted_workspace(root, GIT_TIMEOUT, GIT_CAPTURE_BYTES)
        .map_err(HealthCheckError::Execution)?;
    let maximum = format!("--max-count={count}");
    let spec = CommandSpec::new("git", root, SideEffectClass::PureRead).args([
        "rev-list",
        "--first-parent",
        maximum.as_str(),
        "HEAD",
    ]);
    let result = LocalProcessExecutor
        .execute(&policy, spec)
        .map_err(HealthCheckError::Execution)?;
    if !result.success || result.stdout.truncated {
        return Err(HealthCheckError::Git(
            "git rev-list failed while resolving the health baseline".to_owned(),
        ));
    }

    let revisions = String::from_utf8_lossy(&result.stdout.preview)
        .lines()
        .map(str::trim)
        .filter(|revision| !revision.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let available = revisions.len().saturating_sub(1);
    if available < distance {
        return Err(HealthCheckError::InsufficientHistory {
            requested: distance,
            available,
        });
    }

    Ok(BaselineRevision {
        revision: revisions[distance].clone(),
        first_parent_distance: distance,
    })
}

/// The source files that differ from the given revision.
///
/// Only tracked changes are listed. A file that has never been committed is
/// invisible here, which is correct for a gate run against a merge base — where
/// every file in the change is committed — and a real blind spot when the tool
/// is run against `HEAD` in a working tree with new files.
///
/// # Errors
///
/// Fails when Git cannot run.
pub fn changed_sources(root: &Path, revision: &str) -> Result<Vec<String>, HealthCheckError> {
    let policy = ExecutionPolicy::trusted_workspace(root, GIT_TIMEOUT, GIT_CAPTURE_BYTES)
        .map_err(HealthCheckError::Execution)?;
    let spec = CommandSpec::new("git", root, SideEffectClass::PureRead).args([
        "diff",
        "--name-only",
        revision,
        "--",
        "*.rs",
    ]);
    let result = LocalProcessExecutor
        .execute(&policy, spec)
        .map_err(HealthCheckError::Execution)?;
    if !result.success {
        return Err(HealthCheckError::Git("git diff failed".to_owned()));
    }
    let listing = String::from_utf8_lossy(&result.stdout.preview).into_owned();
    let mut paths = listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// The full report this tool produces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthReport {
    /// Dependencies crossing a forbidden layer boundary.
    pub violations: Vec<BoundaryViolation>,
    /// Source files compared against the baseline revision.
    pub compared: usize,
    /// Every dimension that moved, whether or not it is notable.
    pub findings: Vec<Finding>,
    /// The acceptance decision for the measured change.
    pub verdict: HealthVerdict,
}

impl HealthReport {
    /// Whether the repository gate should fail.
    ///
    /// Only a crossed boundary fails. A worsened metric is reported for a human
    /// to judge, because a threshold that fails a build has to be loose enough
    /// to be useless.
    #[must_use]
    pub fn blocked(&self) -> bool {
        !self.violations.is_empty()
    }
}

/// Runs both checks against a repository.
///
/// # Errors
///
/// Fails when the workspace cannot be read or Git cannot run.
pub fn check_repository(root: &Path, revision: &str) -> Result<HealthReport, HealthCheckError> {
    let members = workspace_members(root)?;
    let dependencies = member_dependencies(root, &members)?;
    let violations = boundary_violations(&workspace_layers(), &dependencies);

    let changed = changed_sources(root, revision)?;
    let before = measure_revision(root, revision, &changed)?;
    let after = measure_working_tree(root, &changed)?;
    let findings = compare(&before, &after);
    let verdict = evaluate(&findings, &violations, &[], HealthPolicy::default(), 0);

    Ok(HealthReport {
        violations,
        compared: changed.len(),
        findings,
        verdict,
    })
}

/// The workspace root, resolved from this crate's manifest location.
#[must_use]
pub fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn measure(path: &str, text: &str) -> Result<FileHealth, HealthCheckError> {
    let measured = measure_source(path, text)
        .map_err(|error| HealthCheckError::Measurement(error.to_string()))?;
    let units = measured
        .units
        .into_iter()
        .map(|unit| UnitSpan {
            name: unit.name,
            start_line: unit.start_line,
            end_line: unit.end_line,
        })
        .collect::<Vec<_>>();
    Ok(FileHealth::measure(measured.lines, &units, measured.tier))
}

fn join_relative(member: &str, relative: &str) -> String {
    let mut segments = normalize(member)
        .split('/')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for segment in normalize(relative).split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other.to_owned()),
        }
    }
    segments.join("/")
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_owned()
}

/// Failures this tool can report.
#[derive(Debug)]
pub enum HealthCheckError {
    /// A manifest or source file could not be read.
    Io(std::io::Error),
    /// A Git command could not be run.
    Execution(ExecutionError),
    /// A Git command ran and failed.
    Git(String),
    /// A file could not be measured by the language adapters.
    Measurement(String),
    /// A zero or overflowing first-parent distance was requested.
    InvalidBaselineDistance,
    /// Git history did not contain the explicitly requested baseline.
    InsufficientHistory {
        /// Requested first-parent distance.
        requested: usize,
        /// First-parent distance available in the checkout.
        available: usize,
    },
}

impl fmt::Display for HealthCheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Execution(error) => error.fmt(formatter),
            Self::Git(message) | Self::Measurement(message) => formatter.write_str(message),
            Self::InvalidBaselineDistance => {
                formatter.write_str("health baseline distance must be greater than zero")
            }
            Self::InsufficientHistory {
                requested,
                available,
            } => write!(
                formatter,
                "health baseline requires {requested} first-parent commit(s), but only {available} are available"
            ),
        }
    }
}

impl std::error::Error for HealthCheckError {}

/// Renders the report the way the command prints it.
#[must_use]
pub fn render(report: &HealthReport) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    if report.violations.is_empty() {
        let _ = writeln!(out, "architecture layers: ok");
    } else {
        let _ = writeln!(
            out,
            "architecture layers: {} forbidden dependency(ies)",
            report.violations.len()
        );
        for violation in &report.violations {
            let _ = writeln!(
                out,
                "  {} ({}) -> {} ({})",
                violation.from_path, violation.from_layer, violation.to_path, violation.to_layer
            );
        }
    }

    let _ = writeln!(
        out,
        "health delta: {} source file(s) compared",
        report.compared
    );
    match &report.verdict {
        HealthVerdict::Accept => {
            let _ = writeln!(out, "  nothing moved enough to report");
        }
        HealthVerdict::Review(notable) => {
            for finding in notable {
                let _ = writeln!(
                    out,
                    "  {} {} {} -> {} [{}]",
                    finding.path.as_deref().unwrap_or("<repository>"),
                    finding.dimension.as_str(),
                    finding.before,
                    finding.after,
                    tier_label(finding),
                );
            }
        }
        HealthVerdict::Block(_) => {
            let _ = writeln!(out, "  blocked by the layer gate above");
        }
    }
    out
}

fn tier_label(finding: &Finding) -> &'static str {
    match finding.tier.as_u8() {
        0 => "text",
        1 => "syntax",
        2 => "project",
        3 => "precise-semantic",
        _ => "dynamic-evidence",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    struct GitFixture {
        root: PathBuf,
    }

    impl GitFixture {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "everything-health-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(root.join("src")).expect("fixture directory");
            let fixture = Self { root };
            fixture.git(&["init", "--quiet"]);
            fixture.git(&["config", "user.name", "Everything Tests"]);
            fixture.git(&["config", "user.email", "tests@everything.invalid"]);
            fixture.git(&["config", "core.autocrlf", "false"]);
            fixture
        }

        fn git(&self, arguments: &[&str]) -> String {
            let output = Command::new("git")
                .args(arguments)
                .current_dir(&self.root)
                .output()
                .expect("git command");
            assert!(
                output.status.success(),
                "git {arguments:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }

        fn commit_hotspot(&self, added_lines: usize) {
            let mut source = String::from("pub fn hotspot() {\n");
            for index in 0..added_lines {
                source.push_str(&format!("    let value_{index} = {index};\n"));
            }
            source.push_str("}\n");
            fs::write(self.root.join("src/lib.rs"), source).expect("fixture source");
            self.git(&["add", "src/lib.rs"]);
            self.git(&["commit", "--quiet", "-m", &format!("growth {added_lines}")]);
        }
    }

    impl Drop for GitFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).expect("fixture cleanup");
        }
    }

    fn verdict_against(root: &Path, revision: &str) -> HealthVerdict {
        let changed = changed_sources(root, revision).expect("changed sources");
        let before = measure_revision(root, revision, &changed).expect("baseline health");
        let after = measure_working_tree(root, &changed).expect("current health");
        let findings = compare(&before, &after);
        evaluate(&findings, &[], &[], HealthPolicy::default(), 0)
    }

    #[test]
    fn this_workspace_has_no_forbidden_crate_dependency() {
        let root = repository_root();
        let members = workspace_members(&root).expect("members");
        assert!(members.len() > 5, "{members:?}");
        let dependencies = member_dependencies(&root, &members).expect("dependencies");
        let violations = boundary_violations(&workspace_layers(), &dependencies);
        assert!(violations.is_empty(), "{violations:#?}");
    }

    #[test]
    fn the_lowest_layer_is_actually_constrained() {
        // A dependency that would invert the architecture must be caught, or the
        // passing assertion above proves nothing.
        let inverted = vec![MemberDependencies {
            directory: "crates/aer-exec".to_owned(),
            depends_on: vec!["crates/aer-core".to_owned()],
        }];
        let violations = boundary_violations(&workspace_layers(), &inverted);
        assert_eq!(violations.len(), 1, "{violations:#?}");
        assert_eq!(violations[0].from_layer, "infrastructure");
        assert_eq!(violations[0].to_layer, "application");
    }

    #[test]
    fn the_domain_layer_may_depend_on_nothing() {
        let violations = boundary_violations(
            &workspace_layers(),
            &[MemberDependencies {
                directory: "crates/aer-domain".to_owned(),
                depends_on: vec!["crates/aer-contracts".to_owned()],
            }],
        );
        assert_eq!(violations.len(), 1, "{violations:#?}");
    }

    #[test]
    fn relative_dependency_paths_resolve_to_workspace_directories() {
        assert_eq!(
            join_relative("crates/aer-core", "../aer-domain"),
            "crates/aer-domain"
        );
        assert_eq!(
            join_relative("tools/aer-health-check", "../../crates/aer-repo"),
            "crates/aer-repo"
        );
    }

    #[test]
    fn every_workspace_member_declares_a_readable_manifest() {
        let root = repository_root();
        let members = workspace_members(&root).expect("members");
        let dependencies = member_dependencies(&root, &members).expect("dependencies");
        assert_eq!(dependencies.len(), members.len());
    }

    #[test]
    fn a_clean_report_is_not_blocked() {
        let report = HealthReport {
            violations: Vec::new(),
            compared: 0,
            findings: Vec::new(),
            verdict: HealthVerdict::Accept,
        };
        assert!(!report.blocked());
        assert!(render(&report).contains("architecture layers: ok"));
    }

    #[test]
    fn a_forbidden_dependency_blocks_and_is_named_in_the_report() {
        let violations = boundary_violations(
            &workspace_layers(),
            &[MemberDependencies {
                directory: "crates/aer-exec".to_owned(),
                depends_on: vec!["crates/aer-core".to_owned()],
            }],
        );
        let report = HealthReport {
            verdict: HealthVerdict::Block(violations.clone()),
            violations,
            compared: 0,
            findings: Vec::new(),
        };
        assert!(report.blocked());
        let rendered = render(&report);
        assert!(rendered.contains("crates/aer-exec"), "{rendered}");
        assert!(rendered.contains("application"), "{rendered}");
    }

    #[test]
    fn a_snapshot_records_duplication_between_the_files_it_measures() {
        let block = (1..=10)
            .map(|index| format!("    let value{index} = compute({index});"))
            .collect::<Vec<_>>()
            .join("\n");
        let sources = vec![
            ("src/a.rs".to_owned(), format!("fn a() {{\n{block}\n}}\n")),
            ("src/b.rs".to_owned(), format!("fn b() {{\n{block}\n}}\n")),
        ];
        let snapshot = snapshot_of(&sources).expect("snapshot");
        assert_eq!(
            snapshot
                .file("src/a.rs")
                .expect("measured")
                .duplicated_lines,
            10
        );
    }

    #[test]
    fn measuring_a_rust_buffer_reuses_the_language_adapters() {
        let health =
            measure("src/lib.rs", "fn only() {\n    let value = 1;\n}\n").expect("measure");
        assert_eq!(health.units, 1);
        assert!(health.lines >= 3);
    }

    #[test]
    fn distance_baseline_exposes_slow_erosion_hidden_by_the_previous_commit() {
        let repository = GitFixture::new("drift");
        repository.commit_hotspot(100);
        for change in 1..=12 {
            repository.commit_hotspot(100 + change * 5);
        }

        let previous = baseline_at_distance(&repository.root, 1).expect("previous baseline");
        let drift = baseline_at_distance(&repository.root, 12).expect("drift baseline");
        let expected = repository.git(&["rev-parse", "HEAD~12"]);

        assert_eq!(drift.revision, expected);
        assert_eq!(drift.first_parent_distance, 12);
        assert_eq!(
            verdict_against(&repository.root, &previous.revision),
            HealthVerdict::Accept,
            "five lines of local growth stay below the review threshold"
        );
        assert!(
            matches!(
                verdict_against(&repository.root, &drift.revision),
                HealthVerdict::Review(_)
            ),
            "sixty lines of cumulative growth must be visible to the drift gate"
        );
    }

    #[test]
    fn distance_baseline_fails_closed_when_history_is_too_shallow() {
        let repository = GitFixture::new("shallow");
        repository.commit_hotspot(0);

        assert!(matches!(
            baseline_at_distance(&repository.root, 1),
            Err(HealthCheckError::InsufficientHistory {
                requested: 1,
                available: 0
            })
        ));
    }

    #[test]
    fn distance_baseline_rejects_zero_distance() {
        let repository = GitFixture::new("zero-distance");
        repository.commit_hotspot(0);

        assert!(matches!(
            baseline_at_distance(&repository.root, 0),
            Err(HealthCheckError::InvalidBaselineDistance)
        ));
    }
}
