//! Architecture health as a time series over the repository.
//!
//! Long-horizon agent work keeps tests passing while a codebase becomes more
//! verbose, more coupled and harder to extend. `docs/18` therefore asks for
//! measurement rather than exhortation, and sets three rules this crate obeys
//! literally.
//!
//! **No aggregate score.** One number cannot capture maintainability, so there
//! is no health score here. A snapshot difference is a list of per-dimension
//! [`Finding`]s and nothing else.
//!
//! **Delta, not absolute thresholds.** A repository that already contains a
//! large file must not block every unrelated patch. Acceptance therefore reads
//! the change between two snapshots; a pre-existing condition that a patch does
//! not worsen produces no finding at all.
//!
//! **Evidence keeps its provenance.** Every finding carries the
//! [`CapabilityTier`] that produced it, so a measurement inferred from a syntax
//! adapter is never presented as compiler truth.
//!
//! What is deliberately absent: duplication, dead code and documentation drift.
//! `docs/18` lists them, but nothing in this repository can measure them today,
//! and a dimension that always reports zero would read as a clean bill of
//! health it did not earn.

use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash as _, Hasher as _};

use aer_repo::CapabilityTier;

/// A unit above this many lines counts toward concentrated complexity mass.
///
/// The value is a policy input rather than a discovered constant: it exists so
/// "how much of this file lives in oversized units" is answerable at all.
/// Concentration is compared against itself over time, so its absolute level
/// matters far less than its direction.
pub const CONCENTRATION_UNIT_LINES: u32 = 60;

/// The health dimensions this crate can actually measure.
///
/// A crossed architecture boundary is deliberately not one of them. It is not a
/// number that moved: it is a declared rule that a dependency broke, and it
/// carries its own type so it can never be averaged in with a metric.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HealthDimension {
    /// Total addressable lines in a file.
    FileLines,
    /// Number of measurable units — functions, methods, types — in a file.
    UnitCount,
    /// Lines in the largest single unit of a file.
    LargestUnitLines,
    /// Share of a file's lines that sit inside oversized units, in basis points.
    ///
    /// This is the structural-erosion signal: appending logic to an existing
    /// hotspot moves it even when the file barely grows.
    ComplexityConcentration,
    /// Lines of a file that repeat a block found elsewhere in the snapshot.
    DuplicatedLines,
}

impl HealthDimension {
    /// The stable identifier recorded in evidence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileLines => "file-lines",
            Self::UnitCount => "unit-count",
            Self::LargestUnitLines => "largest-unit-lines",
            Self::ComplexityConcentration => "complexity-concentration",
            Self::DuplicatedLines => "duplicated-lines",
        }
    }
}

/// One measurable unit inside a file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnitSpan {
    /// Identifier of the unit within its file.
    pub name: String,
    /// First line of the unit, 1-indexed.
    pub start_line: u32,
    /// Last line of the unit, inclusive.
    pub end_line: u32,
}

impl UnitSpan {
    /// Lines the unit occupies, counting both endpoints.
    #[must_use]
    pub const fn lines(&self) -> u32 {
        self.end_line
            .saturating_sub(self.start_line)
            .saturating_add(1)
    }
}

/// The measured health of a single file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileHealth {
    /// Addressable lines in the file.
    pub lines: u32,
    /// Units the adapter could identify.
    pub units: u32,
    /// Lines in the largest unit.
    pub largest_unit_lines: u32,
    /// Share of the file's lines inside oversized units, in basis points.
    pub concentration_bps: u32,
    /// Lines belonging to a block that also appears elsewhere in the snapshot.
    ///
    /// Zero until a duplication scan fills it. Duplication is a property of a
    /// set of files, not of one file, so a single measurement cannot know it.
    pub duplicated_lines: u32,
    /// The evidence tier that produced these numbers.
    pub tier: CapabilityTier,
}

impl FileHealth {
    /// Measures one file from the unit spans an adapter extracted.
    ///
    /// The tier travels with the measurement because the same file measured by
    /// a syntax adapter and by a compiler are different claims, and the
    /// controller must never let the weaker one impersonate the stronger.
    #[must_use]
    pub fn measure(lines: u32, units: &[UnitSpan], tier: CapabilityTier) -> Self {
        let largest_unit_lines = units.iter().map(UnitSpan::lines).max().unwrap_or(0);
        let concentrated: u32 = units
            .iter()
            .map(UnitSpan::lines)
            .filter(|unit_lines| *unit_lines > CONCENTRATION_UNIT_LINES)
            .sum();
        let concentration_bps = if lines == 0 {
            0
        } else {
            u32::try_from(u64::from(concentrated.min(lines)) * 10_000 / u64::from(lines))
                .unwrap_or(10_000)
        };
        Self {
            lines,
            units: u32::try_from(units.len()).unwrap_or(u32::MAX),
            largest_unit_lines,
            concentration_bps,
            duplicated_lines: 0,
            tier,
        }
    }

    /// Records how many of this file's lines repeat a block found elsewhere.
    #[must_use]
    pub const fn with_duplicated_lines(mut self, duplicated_lines: u32) -> Self {
        self.duplicated_lines = duplicated_lines;
        self
    }

    fn value(&self, dimension: HealthDimension) -> u32 {
        match dimension {
            HealthDimension::FileLines => self.lines,
            HealthDimension::UnitCount => self.units,
            HealthDimension::LargestUnitLines => self.largest_unit_lines,
            HealthDimension::ComplexityConcentration => self.concentration_bps,
            HealthDimension::DuplicatedLines => self.duplicated_lines,
        }
    }
}

/// Measured health of a repository at one revision.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HealthSnapshot {
    files: BTreeMap<String, FileHealth>,
}

impl HealthSnapshot {
    /// An empty snapshot, which is what a repository looked like before it existed.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one file's measurement, replacing any earlier one for that path.
    #[must_use]
    pub fn with_file(mut self, path: impl Into<String>, health: FileHealth) -> Self {
        self.files.insert(path.into(), health);
        self
    }

    /// The measurement for one path.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&FileHealth> {
        self.files.get(path)
    }

    /// Every measured path, in deterministic order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

/// Consecutive normalized lines that must match before a block counts as
/// duplicated.
///
/// Short repeats are the shape of a language, not of copied logic: two lines of
/// closing braces are not a maintenance problem. Six is long enough that an
/// accidental match is rare and short enough to catch a copied guard clause.
pub const DUPLICATE_WINDOW_LINES: usize = 6;

/// Finds line blocks that repeat across a set of source buffers.
///
/// The scan is line-based and therefore heuristic: it sees copied text, not
/// copied meaning, and renaming one identifier hides a block from it. That is
/// why the resulting measurement carries the same capability tier as the rest
/// of the file's numbers rather than being presented as a semantic fact.
///
/// Lines are normalized by collapsing whitespace, and lines with almost no
/// content are dropped before windowing, so indentation changes and closing
/// braces neither create nor hide a duplicate.
#[must_use]
pub fn scan_duplication<'a, I>(sources: I) -> BTreeMap<String, u32>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let prepared = sources
        .into_iter()
        .map(|(path, text)| (path.to_owned(), significant_lines(text)))
        .collect::<Vec<_>>();

    let mut occurrences: BTreeMap<u64, u32> = BTreeMap::new();
    for (_, lines) in &prepared {
        for window in lines.windows(DUPLICATE_WINDOW_LINES) {
            *occurrences.entry(window_hash(window)).or_default() += 1;
        }
    }

    let mut duplicated = BTreeMap::new();
    for (path, lines) in &prepared {
        let mut covered = vec![false; lines.len()];
        for (start, window) in lines.windows(DUPLICATE_WINDOW_LINES).enumerate() {
            if occurrences.get(&window_hash(window)).copied().unwrap_or(0) > 1 {
                for flag in covered.iter_mut().skip(start).take(DUPLICATE_WINDOW_LINES) {
                    *flag = true;
                }
            }
        }
        let count = u32::try_from(covered.iter().filter(|flag| **flag).count()).unwrap_or(u32::MAX);
        duplicated.insert(path.clone(), count);
    }
    duplicated
}

fn significant_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| line.chars().filter(|c| c.is_alphanumeric()).count() >= 3)
        .collect()
}

fn window_hash(window: &[String]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for line in window {
        line.hash(&mut hasher);
    }
    hasher.finish()
}

/// One dimension that moved between two snapshots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    /// What moved.
    pub dimension: HealthDimension,
    /// Where it moved, or `None` for a repository-wide finding.
    pub path: Option<String>,
    /// The value before the change.
    pub before: u32,
    /// The value after the change.
    pub after: u32,
    /// The evidence tier behind the measurement.
    pub tier: CapabilityTier,
}

impl Finding {
    /// How much the dimension worsened. Zero when it improved or held.
    #[must_use]
    pub const fn regression(&self) -> u32 {
        self.after.saturating_sub(self.before)
    }
}

/// A dependency that crosses a declared boundary the rules forbid.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BoundaryViolation {
    /// The depending path.
    pub from_path: String,
    /// The depended-upon path.
    pub to_path: String,
    /// Layer the depending path belongs to.
    pub from_layer: String,
    /// Layer the depended-upon path belongs to.
    pub to_layer: String,
}

/// One declared layer and what it may depend on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer {
    name: String,
    prefixes: Vec<String>,
    may_depend_on: BTreeSet<String>,
}

impl Layer {
    /// Declares a layer, the paths it owns, and the layers it may depend on.
    #[must_use]
    pub fn new<P, D>(name: impl Into<String>, prefixes: P, may_depend_on: D) -> Self
    where
        P: IntoIterator,
        P::Item: Into<String>,
        D: IntoIterator,
        D::Item: Into<String>,
    {
        Self {
            name: name.into(),
            prefixes: prefixes.into_iter().map(Into::into).collect(),
            may_depend_on: may_depend_on.into_iter().map(Into::into).collect(),
        }
    }
}

/// Machine-readable architecture boundaries.
///
/// A path that belongs to no declared layer is unconstrained: declaring rules
/// for part of a repository must not silently forbid the rest of it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LayerRules {
    layers: Vec<Layer>,
}

impl LayerRules {
    /// Rules over the given layers, in declaration order.
    ///
    /// Order matters only for overlapping prefixes, where the first declared
    /// layer wins, so a specific path can be carved out of a general one.
    #[must_use]
    pub fn new(layers: impl IntoIterator<Item = Layer>) -> Self {
        Self {
            layers: layers.into_iter().collect(),
        }
    }

    /// The layer owning a path, if any.
    #[must_use]
    pub fn layer_of(&self, path: &str) -> Option<&str> {
        self.layers
            .iter()
            .find(|layer| {
                layer
                    .prefixes
                    .iter()
                    .any(|prefix| path.starts_with(prefix.as_str()))
            })
            .map(|layer| layer.name.as_str())
    }

    /// Every dependency the rules forbid, in deterministic order.
    ///
    /// A dependency inside one layer is always allowed: layering constrains what
    /// crosses a boundary, not what happens within one.
    #[must_use]
    pub fn violations<'a, I>(&self, dependencies: I) -> Vec<BoundaryViolation>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut violations: Vec<BoundaryViolation> = dependencies
            .into_iter()
            .filter_map(|(from_path, to_path)| {
                let from_layer = self.layers.iter().find(|layer| {
                    layer
                        .prefixes
                        .iter()
                        .any(|prefix| from_path.starts_with(prefix.as_str()))
                })?;
                let to_layer = self.layer_of(to_path)?;
                if to_layer == from_layer.name || from_layer.may_depend_on.contains(to_layer) {
                    return None;
                }
                Some(BoundaryViolation {
                    from_path: from_path.to_owned(),
                    to_path: to_path.to_owned(),
                    from_layer: from_layer.name.clone(),
                    to_layer: to_layer.to_owned(),
                })
            })
            .collect();
        violations.sort();
        violations.dedup();
        violations
    }
}

/// Every dimension that moved between two snapshots.
///
/// Files present in only one snapshot are handled asymmetrically on purpose. A
/// new file is measured against zero, because its size is a real addition.
/// A deleted file produces no finding, because removing code is not a
/// regression the controller should argue with.
#[must_use]
pub fn compare(before: &HealthSnapshot, after: &HealthSnapshot) -> Vec<Finding> {
    const DIMENSIONS: [HealthDimension; 5] = [
        HealthDimension::FileLines,
        HealthDimension::UnitCount,
        HealthDimension::LargestUnitLines,
        HealthDimension::ComplexityConcentration,
        HealthDimension::DuplicatedLines,
    ];

    let mut findings = Vec::new();
    for path in after.paths() {
        let Some(current) = after.file(path) else {
            continue;
        };
        let previous = before.file(path);
        for dimension in DIMENSIONS {
            let before_value = previous.map_or(0, |file| file.value(dimension));
            let after_value = current.value(dimension);
            if after_value <= before_value {
                continue;
            }
            findings.push(Finding {
                dimension,
                path: Some(path.to_owned()),
                before: before_value,
                after: after_value,
                tier: current.tier,
            });
        }
    }
    findings
}

/// An explicit, time-bounded acceptance of a regression.
///
/// `docs/18` allows temporary complexity but forbids silent debt, so a record
/// must name what it excuses, for whom, until when, and what will repay it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebtRecord {
    /// Stable identifier of this debt.
    pub id: String,
    /// Why the regression was accepted.
    pub reason: String,
    /// The task or owner accountable for it.
    pub owner_task: String,
    /// The dimension it excuses.
    pub dimension: HealthDimension,
    /// The path it excuses, or `None` for any path.
    pub path: Option<String>,
    /// The largest regression this record covers.
    pub allowed_regression: u32,
    /// The revision count after which the record stops applying.
    pub expires_at_revision: u64,
    /// What will repay the debt.
    pub planned_remediation: String,
}

impl DebtRecord {
    fn covers(&self, finding: &Finding, revision: u64) -> bool {
        if revision >= self.expires_at_revision || self.dimension != finding.dimension {
            return false;
        }
        if self
            .path
            .as_deref()
            .is_some_and(|path| Some(path) != finding.path.as_deref())
        {
            return false;
        }
        finding.regression() <= self.allowed_regression
    }
}

/// How much movement acceptance tolerates before it wants a human.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HealthPolicy {
    /// Growth in a single unit, in lines, that still passes unremarked.
    pub unit_lines_review_above: u32,
    /// Rise in concentration, in basis points, that still passes unremarked.
    pub concentration_review_above_bps: u32,
    /// Growth in duplicated lines that still passes unremarked.
    pub duplicated_lines_review_above: u32,
    /// Whether a forbidden dependency blocks rather than asks.
    pub boundary_violation_blocks: bool,
}

impl Default for HealthPolicy {
    /// A deliberately unambitious default.
    ///
    /// It is loose enough that ordinary work passes silently and tight enough
    /// that a hotspot growing by a third of a screen is noticed. These numbers
    /// are policy, not measurement, and are expected to be tuned against a
    /// repository's own history rather than defended as universal.
    fn default() -> Self {
        Self {
            unit_lines_review_above: 40,
            concentration_review_above_bps: 500,
            duplicated_lines_review_above: 24,
            boundary_violation_blocks: true,
        }
    }
}

/// What acceptance should do with a change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthVerdict {
    /// Nothing moved enough to mention.
    Accept,
    /// Something moved and a human should look.
    Review(Vec<Finding>),
    /// Something crossed a boundary the rules forbid.
    Block(Vec<BoundaryViolation>),
}

/// The acceptance decision for one change.
///
/// Findings covered by an active debt record are excluded from review: that is
/// what recording the debt bought. Expired records buy nothing, which is how a
/// time-bounded record stays time-bounded.
#[must_use]
pub fn evaluate(
    findings: &[Finding],
    violations: &[BoundaryViolation],
    debts: &[DebtRecord],
    policy: HealthPolicy,
    revision: u64,
) -> HealthVerdict {
    if policy.boundary_violation_blocks && !violations.is_empty() {
        return HealthVerdict::Block(violations.to_vec());
    }

    let threshold = |dimension: HealthDimension| match dimension {
        HealthDimension::LargestUnitLines => policy.unit_lines_review_above,
        HealthDimension::ComplexityConcentration => policy.concentration_review_above_bps,
        HealthDimension::DuplicatedLines => policy.duplicated_lines_review_above,
        // File and unit counts move for every honest feature. They are recorded
        // for the time series but do not by themselves ask for a human.
        _ => u32::MAX,
    };

    let notable = findings
        .iter()
        .filter(|finding| finding.regression() > threshold(finding.dimension))
        .filter(|finding| !debts.iter().any(|debt| debt.covers(finding, revision)))
        .cloned()
        .collect::<Vec<_>>();

    if notable.is_empty() {
        HealthVerdict::Accept
    } else {
        HealthVerdict::Review(notable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(name: &str, start_line: u32, end_line: u32) -> UnitSpan {
        UnitSpan {
            name: name.to_owned(),
            start_line,
            end_line,
        }
    }

    fn snapshot(path: &str, lines: u32, units: &[UnitSpan]) -> HealthSnapshot {
        HealthSnapshot::new().with_file(
            path,
            FileHealth::measure(lines, units, CapabilityTier::Tier1Syntax),
        )
    }

    #[test]
    fn duplication_finds_a_block_copied_between_two_files() {
        let block = "let first = compute(1);\nlet second = compute(2);\nlet third = compute(3);\nlet fourth = compute(4);\nlet fifth = compute(5);\nlet sixth = compute(6);\n";
        let scan = scan_duplication([
            ("src/a.rs", format!("fn a() {{\n{block}}}\n").as_str()),
            ("src/b.rs", format!("fn b() {{\n{block}}}\n").as_str()),
        ]);
        assert_eq!(scan.get("src/a.rs").copied(), Some(6));
        assert_eq!(scan.get("src/b.rs").copied(), Some(6));
    }

    #[test]
    fn duplication_ignores_repeats_shorter_than_the_window() {
        let scan = scan_duplication([
            (
                "src/a.rs",
                "let value = compute(1);\nlet other = compute(2);\n",
            ),
            (
                "src/b.rs",
                "let value = compute(1);\nlet other = compute(2);\n",
            ),
        ]);
        assert_eq!(scan.get("src/a.rs").copied(), Some(0));
    }

    #[test]
    fn duplication_is_blind_to_indentation_but_not_to_content() {
        let block = |indent: &str| {
            (1..=6)
                .map(|index| format!("{indent}let value{index} = compute({index});"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let indented = block("        ");
        let flat = block("");
        let scan = scan_duplication([("src/a.rs", indented.as_str()), ("src/b.rs", flat.as_str())]);
        assert_eq!(scan.get("src/a.rs").copied(), Some(6));

        let different = (1..=6)
            .map(|index| format!("let value{index} = other({index});"))
            .collect::<Vec<_>>()
            .join("\n");
        let scan = scan_duplication([
            ("src/a.rs", flat.as_str()),
            ("src/b.rs", different.as_str()),
        ]);
        assert_eq!(scan.get("src/a.rs").copied(), Some(0));
    }

    #[test]
    fn copying_a_block_into_a_new_file_asks_for_review() {
        let block = (1..=30)
            .map(|index| format!("let value{index} = compute({index});"))
            .collect::<Vec<_>>()
            .join("\n");
        let before = HealthSnapshot::new().with_file(
            "src/a.rs",
            FileHealth::measure(20, &[unit("a", 1, 10)], CapabilityTier::Tier1Syntax),
        );
        let scan = scan_duplication([("src/a.rs", block.as_str()), ("src/b.rs", block.as_str())]);
        let after = before.clone().with_file(
            "src/a.rs",
            FileHealth::measure(20, &[unit("a", 1, 10)], CapabilityTier::Tier1Syntax)
                .with_duplicated_lines(scan["src/a.rs"]),
        );
        let findings = compare(&before, &after);
        assert!(matches!(
            evaluate(&findings, &[], &[], HealthPolicy::default(), 1),
            HealthVerdict::Review(_)
        ));
    }

    #[test]
    fn concentration_measures_the_share_of_a_file_inside_oversized_units() {
        let health = FileHealth::measure(
            200,
            &[unit("small", 1, 20), unit("hotspot", 21, 120)],
            CapabilityTier::Tier1Syntax,
        );
        assert_eq!(health.units, 2);
        assert_eq!(health.largest_unit_lines, 100);
        assert_eq!(health.concentration_bps, 5000);
    }

    #[test]
    fn a_file_with_no_measurable_unit_reports_no_concentration() {
        let health = FileHealth::measure(120, &[], CapabilityTier::Tier0Text);
        assert_eq!(health.units, 0);
        assert_eq!(health.largest_unit_lines, 0);
        assert_eq!(health.concentration_bps, 0);
    }

    #[test]
    fn appending_to_a_hotspot_is_caught_even_when_the_file_barely_grows() {
        // The file gains ten lines; a small unit is folded into the large one.
        let before = snapshot(
            "src/engine.rs",
            300,
            &[unit("dispatch", 1, 150), unit("helper", 151, 190)],
        );
        let after = snapshot("src/engine.rs", 310, &[unit("dispatch", 1, 200)]);

        let findings = compare(&before, &after);
        let concentration = findings
            .iter()
            .find(|finding| finding.dimension == HealthDimension::ComplexityConcentration)
            .expect("erosion must surface as a concentration finding");
        assert_eq!(concentration.before, 5000);
        assert_eq!(concentration.after, 6451);

        // Unit count fell, so the naive "more units, more complexity" reading
        // would have called this an improvement.
        assert!(
            !findings
                .iter()
                .any(|finding| finding.dimension == HealthDimension::UnitCount)
        );
    }

    #[test]
    fn a_pre_existing_large_file_does_not_block_an_unrelated_patch() {
        let huge = snapshot("src/legacy.rs", 4000, &[unit("everything", 1, 3900)]);
        let after = huge.clone().with_file(
            "src/new.rs",
            FileHealth::measure(30, &[unit("small", 1, 20)], CapabilityTier::Tier1Syntax),
        );

        let findings = compare(&huge, &after);
        assert!(
            findings
                .iter()
                .all(|finding| finding.path.as_deref() == Some("src/new.rs")),
            "{findings:?}"
        );
        assert_eq!(
            evaluate(&findings, &[], &[], HealthPolicy::default(), 1),
            HealthVerdict::Accept
        );
    }

    #[test]
    fn materially_worsening_a_hotspot_asks_for_review() {
        let before = snapshot("src/engine.rs", 300, &[unit("dispatch", 1, 100)]);
        let after = snapshot("src/engine.rs", 400, &[unit("dispatch", 1, 200)]);
        let findings = compare(&before, &after);
        let HealthVerdict::Review(notable) =
            evaluate(&findings, &[], &[], HealthPolicy::default(), 1)
        else {
            panic!("a unit that doubled must not pass unremarked: {findings:?}");
        };
        assert!(
            notable
                .iter()
                .any(|finding| finding.dimension == HealthDimension::LargestUnitLines)
        );
    }

    #[test]
    fn deleting_code_is_never_a_regression() {
        let before = snapshot("src/engine.rs", 400, &[unit("dispatch", 1, 200)]);
        let after = HealthSnapshot::new();
        assert!(compare(&before, &after).is_empty());
    }

    #[test]
    fn every_finding_carries_the_tier_that_produced_it() {
        let before = HealthSnapshot::new();
        let after = HealthSnapshot::new().with_file(
            "src/parsed_by_grammar.rs",
            FileHealth::measure(100, &[unit("f", 1, 90)], CapabilityTier::Tier1Syntax),
        );
        let findings = compare(&before, &after);
        assert!(!findings.is_empty());
        assert!(
            findings
                .iter()
                .all(|finding| finding.tier == CapabilityTier::Tier1Syntax),
            "a syntax-derived measurement must not claim a stronger tier"
        );
    }

    fn layered() -> LayerRules {
        LayerRules::new([
            Layer::new("domain", ["crates/aer-domain/"], Vec::<String>::new()),
            Layer::new("application", ["crates/aer-core/"], ["domain"]),
            Layer::new(
                "infrastructure",
                ["crates/aer-storage/", "crates/aer-exec/"],
                ["domain", "application"],
            ),
        ])
    }

    #[test]
    fn a_dependency_the_rules_forbid_is_reported_with_both_layers() {
        let violations = layered().violations([(
            "crates/aer-domain/src/lib.rs",
            "crates/aer-storage/src/lib.rs",
        )]);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].from_layer, "domain");
        assert_eq!(violations[0].to_layer, "infrastructure");
    }

    #[test]
    fn allowed_and_intra_layer_dependencies_produce_nothing() {
        let violations = layered().violations([
            ("crates/aer-core/src/a.rs", "crates/aer-domain/src/lib.rs"),
            ("crates/aer-core/src/a.rs", "crates/aer-core/src/b.rs"),
        ]);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn paths_outside_every_declared_layer_stay_unconstrained() {
        let violations = layered().violations([
            (
                "tools/aer-bench/src/main.rs",
                "crates/aer-domain/src/lib.rs",
            ),
            (
                "crates/aer-domain/src/lib.rs",
                "tools/aer-bench/src/main.rs",
            ),
        ]);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn a_forbidden_dependency_blocks_regardless_of_metric_movement() {
        let violations =
            layered().violations([("crates/aer-domain/src/lib.rs", "crates/aer-exec/src/lib.rs")]);
        assert!(matches!(
            evaluate(&[], &violations, &[], HealthPolicy::default(), 1),
            HealthVerdict::Block(_)
        ));
    }

    fn debt(allowed_regression: u32, expires_at_revision: u64) -> DebtRecord {
        DebtRecord {
            id: "debt-1".to_owned(),
            reason: "temporary duplication while the second adapter lands".to_owned(),
            owner_task: "task-42".to_owned(),
            dimension: HealthDimension::LargestUnitLines,
            path: Some("src/engine.rs".to_owned()),
            allowed_regression,
            expires_at_revision,
            planned_remediation: "extract the shared branch once both adapters exist".to_owned(),
        }
    }

    #[test]
    fn an_active_debt_record_excuses_only_the_dimension_it_names() {
        let before = snapshot("src/engine.rs", 300, &[unit("dispatch", 1, 100)]);
        let after = snapshot("src/engine.rs", 400, &[unit("dispatch", 1, 200)]);
        let findings = compare(&before, &after);
        let HealthVerdict::Review(notable) =
            evaluate(&findings, &[], &[debt(100, 10)], HealthPolicy::default(), 5)
        else {
            panic!("concentration also regressed and no record named it: {findings:?}");
        };
        assert_eq!(notable.len(), 1);
        assert_eq!(
            notable[0].dimension,
            HealthDimension::ComplexityConcentration
        );
    }

    #[test]
    fn recording_every_regression_a_change_causes_lets_it_through() {
        let before = snapshot("src/engine.rs", 300, &[unit("dispatch", 1, 100)]);
        let after = snapshot("src/engine.rs", 400, &[unit("dispatch", 1, 200)]);
        let findings = compare(&before, &after);
        let concentration = DebtRecord {
            id: "debt-2".to_owned(),
            dimension: HealthDimension::ComplexityConcentration,
            allowed_regression: 2_000,
            ..debt(100, 10)
        };
        assert_eq!(
            evaluate(
                &findings,
                &[],
                &[debt(100, 10), concentration],
                HealthPolicy::default(),
                5
            ),
            HealthVerdict::Accept
        );
    }

    #[test]
    fn an_expired_debt_record_excuses_nothing() {
        let before = snapshot("src/engine.rs", 300, &[unit("dispatch", 1, 100)]);
        let after = snapshot("src/engine.rs", 400, &[unit("dispatch", 1, 200)]);
        let findings = compare(&before, &after);
        assert!(matches!(
            evaluate(
                &findings,
                &[],
                &[debt(100, 10)],
                HealthPolicy::default(),
                10
            ),
            HealthVerdict::Review(_)
        ));
    }

    #[test]
    fn a_debt_record_does_not_cover_a_larger_regression_than_it_named() {
        let before = snapshot("src/engine.rs", 300, &[unit("dispatch", 1, 100)]);
        let after = snapshot("src/engine.rs", 600, &[unit("dispatch", 1, 400)]);
        let findings = compare(&before, &after);
        assert!(matches!(
            evaluate(&findings, &[], &[debt(100, 10)], HealthPolicy::default(), 1),
            HealthVerdict::Review(_)
        ));
    }

    #[test]
    fn a_debt_record_bound_to_one_path_does_not_travel_to_another() {
        let before = snapshot("src/other.rs", 300, &[unit("dispatch", 1, 100)]);
        let after = snapshot("src/other.rs", 400, &[unit("dispatch", 1, 200)]);
        let findings = compare(&before, &after);
        assert!(matches!(
            evaluate(&findings, &[], &[debt(100, 10)], HealthPolicy::default(), 1),
            HealthVerdict::Review(_)
        ));
    }
}
