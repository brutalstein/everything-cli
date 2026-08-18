//! EvolutionBench — does architecture-health gating slow long-horizon decay?
//!
//! The exit gate for the Architecture Health Controller asks for comparative
//! evidence, not for a controller that merely runs. This harness produces that
//! comparison deterministically and offline: it replays the same synthetic
//! engineering trajectory under several gating regimes and records what each one
//! did to the resulting codebase.
//!
//! # What is real here and what is modelled
//!
//! **Real.** The measurement is the shipped controller. Each regime renders
//! actual source text and measures it with `aer_health::FileHealth::measure`,
//! `scan_duplication` and `evaluate`. A bug in the controller shows up here.
//!
//! **Modelled.** The engineer is a rule, not a language model. It appends logic
//! to the path of least resistance — the largest unit it has already touched —
//! and, when a gate objects, extracts a new unit instead. That is a caricature
//! of one real failure mode, chosen because it is the failure mode `docs/18`
//! names.
//!
//! So this benchmark measures **the gate's effect on a modelled behavior**. It
//! is not evidence about real model trajectories, and the receipt says so. What
//! it can honestly settle is narrower and still worth settling: whether gating
//! changes the trajectory at all, and what it costs when it does.
//!
//! # Why it can lose
//!
//! Extraction is not free. A gated regime pays for every redirect in new units,
//! extra total lines, and a duplicated call preamble at each extraction site.
//! Duplication can therefore end up *worse* under gating, and a policy tuned too
//! tightly redirects work it should have allowed. Both effects are measured and
//! reported rather than hidden, and no regime is declared a winner in code.

use std::collections::BTreeMap;

use aer_health::{
    FileHealth, HealthPolicy, HealthSnapshot, HealthVerdict, UnitSpan, compare, evaluate,
    scan_duplication,
};
use aer_repo::CapabilityTier;
use serde_json::{Value, json};

/// Benchmark identity recorded in every receipt.
const VERSION: &str = "aer-evolution-bench-v1";

/// Receipt schema version.
const SCHEMA_VERSION: u32 = 1;

/// Files in the synthetic project.
const FILES: usize = 6;

/// Engineering tasks replayed per regime.
const TASKS: usize = 120;

/// Lines of logic each task adds.
const TASK_LINES: usize = 9;

/// Lines a unit starts with.
const SEED_UNIT_LINES: usize = 8;

/// Lines of call preamble an extraction leaves behind at the original site.
///
/// Extracting a helper does not delete work, it moves it and leaves a call. The
/// preamble is textually the same at every extraction site, which is exactly how
/// real extraction creates duplication, so the gated regimes pay for it.
const EXTRACTION_PREAMBLE_LINES: usize = 7;

/// Fixed seed. Recorded in the receipt so a rerun is comparable.
const SEED: u64 = 0x5eed_1234_9876_abcd;

/// How far back a drift-aware regime looks for its baseline, in tasks.
///
/// The first run of this benchmark produced a negative result worth keeping:
/// with a one-task baseline, the shipped default policy redirected six changes
/// out of a hundred and twenty and barely moved the outcome. Erosion arrives
/// nine lines at a time, and nine is under every threshold. A gate that only
/// ever compares a change to the change before it cannot see drift — the
/// baseline matters more than the thresholds. This regime exists to test that
/// explanation rather than to assert it.
const DRIFT_BASELINE_TASKS: usize = 12;

/// One unit of synthetic source.
#[derive(Clone)]
struct Unit {
    name: String,
    body: Vec<String>,
}

/// One synthetic source file.
#[derive(Clone)]
struct SourceFile {
    path: String,
    units: Vec<Unit>,
}

impl SourceFile {
    fn render(&self) -> (String, Vec<UnitSpan>, u32) {
        let mut text = String::new();
        let mut spans = Vec::new();
        let mut line = 1_u32;
        for unit in &self.units {
            let start = line;
            text.push_str(&format!("fn {}() {{\n", unit.name));
            line += 1;
            for body_line in &unit.body {
                text.push_str(body_line);
                text.push('\n');
                line += 1;
            }
            text.push_str("}\n");
            line += 1;
            spans.push(UnitSpan {
                name: unit.name.clone(),
                start_line: start,
                end_line: line - 1,
            });
        }
        (text, spans, line.saturating_sub(1))
    }
}

/// The whole synthetic project.
#[derive(Clone)]
struct Project {
    files: Vec<SourceFile>,
    extractions: usize,
}

impl Project {
    fn seeded() -> Self {
        let files = (0..FILES)
            .map(|index| SourceFile {
                path: format!("src/module{index}.rs"),
                units: vec![Unit {
                    name: format!("entry{index}"),
                    body: (0..SEED_UNIT_LINES)
                        .map(|line| format!("    let seed{index}_{line} = base({line});"))
                        .collect(),
                }],
            })
            .collect();
        Self {
            files,
            extractions: 0,
        }
    }

    fn snapshot(&self) -> HealthSnapshot {
        let rendered = self
            .files
            .iter()
            .map(|file| {
                let (text, spans, lines) = file.render();
                (file.path.clone(), text, spans, lines)
            })
            .collect::<Vec<_>>();
        let duplication = scan_duplication(
            rendered
                .iter()
                .map(|(path, text, _, _)| (path.as_str(), text.as_str())),
        );
        let mut snapshot = HealthSnapshot::new();
        for (path, _, spans, lines) in &rendered {
            let health = FileHealth::measure(*lines, spans, CapabilityTier::Tier1Syntax)
                .with_duplicated_lines(duplication.get(path).copied().unwrap_or(0));
            snapshot = snapshot.with_file(path.clone(), health);
        }
        snapshot
    }

    /// Appends work to the largest unit of the chosen file.
    fn append(&mut self, file_index: usize, task: usize) {
        let file = &mut self.files[file_index];
        let target = file
            .units
            .iter_mut()
            .max_by_key(|unit| unit.body.len())
            .expect("a file always has a unit");
        for line in 0..TASK_LINES {
            target
                .body
                .push(format!("    let step{task}_{line} = work({task}, {line});"));
        }
    }

    /// Extracts the work into a new unit, leaving a call preamble behind.
    fn extract(&mut self, file_index: usize, task: usize) {
        let file = &mut self.files[file_index];
        let name = format!("extracted{task}");
        file.units.push(Unit {
            name: name.clone(),
            body: (0..TASK_LINES)
                .map(|line| format!("    let step{task}_{line} = work({task}, {line});"))
                .collect(),
        });
        let target = file
            .units
            .iter_mut()
            .filter(|unit| unit.name != name)
            .max_by_key(|unit| unit.body.len())
            .expect("a file always has a unit");
        // The preamble is deliberately identical at every extraction site.
        for line in 0..EXTRACTION_PREAMBLE_LINES {
            target.body.push(format!(
                "    let prepared_argument_{line} = prepare_call_argument({line});"
            ));
        }
        self.extractions += 1;
    }
}

/// A deterministic sequence, so a rerun replays the same trajectory.
struct Sequence(u64);

impl Sequence {
    fn next(&mut self, modulo: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).unwrap_or(0) % modulo
    }
}

/// One gating regime.
struct Regime {
    name: &'static str,
    description: &'static str,
    policy: Option<HealthPolicy>,
    /// How many tasks back the baseline sits. One means the previous change.
    baseline_lag: usize,
}

/// What a regime produced.
struct RegimeOutcome {
    name: &'static str,
    description: &'static str,
    redirects: usize,
    max_unit_lines: u32,
    worst_concentration_bps: u32,
    total_lines: u32,
    total_units: u32,
    duplicated_lines: u32,
}

impl RegimeOutcome {
    fn to_json(&self) -> Value {
        json!({
            "regime": self.name,
            "description": self.description,
            "redirects": self.redirects,
            "max_unit_lines": self.max_unit_lines,
            "worst_concentration_bps": self.worst_concentration_bps,
            "total_lines": self.total_lines,
            "total_units": self.total_units,
            "duplicated_lines": self.duplicated_lines,
        })
    }
}

fn regimes() -> Vec<Regime> {
    vec![
        Regime {
            name: "ungated",
            description: "every change is applied; the control trajectory",
            policy: None,
            baseline_lag: 1,
        },
        Regime {
            name: "gated-default",
            description: "the shipped default policy against the previous change",
            policy: Some(HealthPolicy::default()),
            baseline_lag: 1,
        },
        Regime {
            name: "gated-default-drift",
            description: "the same default policy against a baseline twelve tasks back",
            policy: Some(HealthPolicy::default()),
            baseline_lag: DRIFT_BASELINE_TASKS,
        },
        Regime {
            name: "gated-tight",
            description: "a deliberately over-tight policy, to show that the policy is a choice",
            policy: Some(HealthPolicy {
                unit_lines_review_above: 0,
                concentration_review_above_bps: 0,
                duplicated_lines_review_above: 0,
                boundary_violation_blocks: true,
            }),
            baseline_lag: 1,
        },
    ]
}

fn run(regime: &Regime) -> RegimeOutcome {
    let mut project = Project::seeded();
    let mut sequence = Sequence(SEED);
    let mut redirects = 0;
    let mut history = vec![project.snapshot()];

    for task in 0..TASKS {
        let file_index = sequence.next(FILES);
        let Some(policy) = regime.policy else {
            project.append(file_index, task);
            continue;
        };

        // The baseline is the state `baseline_lag` tasks back, which is what a
        // real acceptance gate compares against when several changes have
        // landed since the branch point.
        let baseline = &history[history.len().saturating_sub(regime.baseline_lag.max(1))];
        let mut candidate = project.clone();
        candidate.append(file_index, task);
        let findings = compare(baseline, &candidate.snapshot());
        // A run under gating sees exactly what acceptance would see.
        match evaluate(&findings, &[], &[], policy, task as u64) {
            HealthVerdict::Accept => project = candidate,
            HealthVerdict::Review(_) | HealthVerdict::Block(_) => {
                project.extract(file_index, task);
                redirects += 1;
            }
        }
        history.push(project.snapshot());
    }

    let snapshot = project.snapshot();
    let mut max_unit_lines = 0;
    let mut worst_concentration_bps = 0;
    let mut total_lines = 0;
    let mut total_units = 0;
    let mut duplicated_lines = 0;
    for path in snapshot.paths().map(str::to_owned).collect::<Vec<_>>() {
        let file = snapshot.file(&path).expect("measured file");
        max_unit_lines = max_unit_lines.max(file.largest_unit_lines);
        worst_concentration_bps = worst_concentration_bps.max(file.concentration_bps);
        total_lines += file.lines;
        total_units += file.units;
        duplicated_lines += file.duplicated_lines;
    }

    RegimeOutcome {
        name: regime.name,
        description: regime.description,
        redirects,
        max_unit_lines,
        worst_concentration_bps,
        total_lines,
        total_units,
        duplicated_lines,
    }
}

fn receipt(outcomes: &[RegimeOutcome]) -> Value {
    let by_name = outcomes
        .iter()
        .map(|outcome| (outcome.name, outcome))
        .collect::<BTreeMap<_, _>>();
    let compare_to_control = |compared: &str| {
        let control = by_name.get("ungated")?;
        let gated = by_name.get(compared)?;
        Some(json!({
                "baseline": control.name,
                "compared": gated.name,
                "max_unit_lines_delta": i64::from(gated.max_unit_lines) - i64::from(control.max_unit_lines),
                "worst_concentration_bps_delta": i64::from(gated.worst_concentration_bps) - i64::from(control.worst_concentration_bps),
                "total_lines_delta": i64::from(gated.total_lines) - i64::from(control.total_lines),
                "total_units_delta": i64::from(gated.total_units) - i64::from(control.total_units),
                "duplicated_lines_delta": i64::from(gated.duplicated_lines) - i64::from(control.duplicated_lines),
        }))
    };
    let comparisons = outcomes
        .iter()
        .filter(|outcome| outcome.name != "ungated")
        .filter_map(|outcome| compare_to_control(outcome.name))
        .collect::<Vec<_>>();

    json!({
        "benchmark": VERSION,
        "schema_version": SCHEMA_VERSION,
        "seed": format!("{SEED:#x}"),
        "files": FILES,
        "tasks": TASKS,
        "regimes": outcomes.iter().map(RegimeOutcome::to_json).collect::<Vec<_>>(),
        "comparisons": comparisons,
        "validity_limits": [
            "The engineer is a deterministic rule, not a language model. This measures the gate's effect on one modelled failure mode, not on real model trajectories.",
            "The project is synthetic. Absolute magnitudes carry no meaning; only the difference between regimes on the same trajectory does.",
            "A lower number is not automatically better. Gating buys structure by adding units, lines and duplicated call preambles, and every one of those costs is reported here.",
            "No regime is declared a winner by this harness. Read the deltas.",
            "Redirect counts are part of the result. A regime that redirected almost nothing did not protect anything, however good its final numbers look."
        ]
    })
}

fn main() {
    let outcomes = regimes().iter().map(run).collect::<Vec<_>>();
    let receipt = receipt(&outcomes);
    println!(
        "{}",
        serde_json::to_string_pretty(&receipt).expect("receipt serializes")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trajectory_is_deterministic() {
        let first = serde_json::to_string(&receipt(&regimes().iter().map(run).collect::<Vec<_>>()))
            .expect("first");
        let second =
            serde_json::to_string(&receipt(&regimes().iter().map(run).collect::<Vec<_>>()))
                .expect("second");
        assert_eq!(first, second, "the same seed must replay the same run");
    }

    #[test]
    fn the_control_regime_never_redirects() {
        let control = run(&regimes()[0]);
        assert_eq!(control.name, "ungated");
        assert_eq!(control.redirects, 0);
        assert!(control.max_unit_lines > 0);
    }

    #[test]
    fn every_regime_reports_every_dimension() {
        for outcome in regimes().iter().map(run) {
            let value = outcome.to_json();
            for key in [
                "max_unit_lines",
                "worst_concentration_bps",
                "total_lines",
                "total_units",
                "duplicated_lines",
            ] {
                assert!(
                    value.get(key).and_then(Value::as_u64).is_some(),
                    "{} omitted {key}",
                    outcome.name
                );
            }
        }
    }

    #[test]
    fn the_receipt_states_its_validity_limits() {
        let value = receipt(&regimes().iter().map(run).collect::<Vec<_>>());
        let limits = value["validity_limits"]
            .as_array()
            .expect("validity limits are part of the receipt");
        assert!(limits.len() >= 4);
        assert!(
            limits
                .iter()
                .filter_map(Value::as_str)
                .any(|limit| limit.contains("not a language model")),
            "the modelled-engineer limit must be stated in the receipt itself"
        );
    }

    #[test]
    fn every_comparison_reports_costs_as_well_as_benefits() {
        // A comparison that could only ever show improvement would be a
        // decoration. Every dimension is reported signed, including the ones
        // gating is expected to worsen.
        let value = receipt(&regimes().iter().map(run).collect::<Vec<_>>());
        let comparisons = value["comparisons"].as_array().expect("comparisons");
        assert_eq!(comparisons.len(), regimes().len() - 1);
        for comparison in comparisons {
            for key in [
                "max_unit_lines_delta",
                "worst_concentration_bps_delta",
                "total_lines_delta",
                "total_units_delta",
                "duplicated_lines_delta",
            ] {
                assert!(
                    comparison.get(key).and_then(Value::as_i64).is_some(),
                    "{key} missing from {comparison}"
                );
            }
        }
    }

    #[test]
    fn a_further_baseline_sees_drift_the_previous_change_hides() {
        // The negative result this benchmark first produced said the thresholds
        // were not the problem. If a further-back baseline does not redirect
        // more than a one-task baseline under the same policy, that explanation
        // is wrong and the note on DRIFT_BASELINE_TASKS must be corrected.
        let outcomes = regimes().iter().map(run).collect::<Vec<_>>();
        let per_change = outcomes
            .iter()
            .find(|outcome| outcome.name == "gated-default")
            .expect("regime");
        let drift = outcomes
            .iter()
            .find(|outcome| outcome.name == "gated-default-drift")
            .expect("regime");
        assert!(
            drift.redirects > per_change.redirects,
            "same policy, further baseline: {} vs {} redirects",
            drift.redirects,
            per_change.redirects
        );
    }

    #[test]
    fn extraction_is_not_free() {
        // The redirect path must cost something, or the benchmark cannot lose.
        let mut project = Project::seeded();
        let before = project.snapshot();
        project.extract(0, 1);
        let after = project.snapshot();
        let before_lines: u32 = before
            .paths()
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .iter()
            .filter_map(|path| before.file(path))
            .map(|file| file.lines)
            .sum();
        let after_lines: u32 = after
            .paths()
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .iter()
            .filter_map(|path| after.file(path))
            .map(|file| file.lines)
            .sum();
        assert!(
            after_lines > before_lines + u32::try_from(TASK_LINES).expect("small"),
            "extraction must add more than the work itself: {before_lines} -> {after_lines}"
        );
    }
}
