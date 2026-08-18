//! Architecture health at task acceptance.
//!
//! `docs/18` asks for two things beyond measurement. Acceptance policy must
//! consider the health delta, and the controller may create a refactoring task
//! when work repeatedly concentrates in a hotspot. This module is where the
//! measurement in `aer-health` becomes a decision with a durable record.
//!
//! Three rules shape it.
//!
//! **The outcome is always journalled.** A health series that only records bad
//! news cannot show a trend, and a repeated-hotspot trigger needs history it can
//! count. Every acceptance appends one event, including the clean ones.
//!
//! **A crossed boundary refuses acceptance.** Layering is a declared fact, not a
//! metric, so a change that breaks it is not accepted with a note.
//!
//! **A refactoring task is created from repetition, not from one bad patch.**
//! One regression is often the honest cost of a feature. The same file and the
//! same dimension regressing again and again is the pattern the controller
//! exists to catch, and the journal is what proves it happened.

use aer_health::{BoundaryViolation, Finding, HealthVerdict};
use aer_storage::{DurableState, EventPayload, NewEvent, StoredEvent};
use serde_json::{Value, json};

/// Journal event recording one acceptance-time health outcome.
pub const HEALTH_EVENT: &str = "architecture.health.recorded";

/// Journal event recording a refactoring task the controller created.
pub const REFACTORING_EVENT: &str = "architecture.health.refactoring_required";

/// How many times one path and dimension must regress before the controller
/// stops accepting the explanation and asks for a refactor.
///
/// Two is a coincidence. Three is a direction.
pub const REPEATED_REGRESSION_THRESHOLD: usize = 3;

/// What acceptance should do after the health outcome is recorded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthAcceptance {
    /// Nothing moved enough to act on. Acceptance may proceed.
    Clear,
    /// Something moved and was recorded. Acceptance may proceed.
    Recorded(Vec<Finding>),
    /// A hotspot has now regressed repeatedly. Acceptance may proceed, and the
    /// controller has created a refactoring task against it.
    RefactoringRequired(Vec<RefactoringTask>),
    /// A declared boundary was crossed. Acceptance must not proceed.
    Blocked(Vec<BoundaryViolation>),
}

impl HealthAcceptance {
    /// Whether the task may be accepted.
    #[must_use]
    pub const fn permits_acceptance(&self) -> bool {
        !matches!(self, Self::Blocked(_))
    }
}

/// A refactoring task the controller created from repeated regression.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefactoringTask {
    /// Deterministic identifier, stable for one path and dimension.
    ///
    /// Stability is the point: a hotspot that keeps regressing must keep
    /// pointing at the same task instead of spawning a new one each time.
    pub task_id: String,
    /// The file the controller wants restructured.
    pub target_path: String,
    /// The dimension that kept regressing.
    pub dimension: String,
    /// How many recorded regressions produced this task.
    pub observed_regressions: usize,
    /// The task whose acceptance created it.
    pub triggering_task_id: String,
}

/// Records an acceptance-time health outcome and decides what follows.
///
/// # Errors
///
/// Fails when the journal cannot be read or appended to.
pub fn record_health_outcome(
    state: &mut DurableState,
    project_id: &str,
    run_id: Option<&str>,
    task_id: &str,
    verdict: &HealthVerdict,
) -> Result<HealthAcceptance, HealthAcceptanceError> {
    if project_id.trim().is_empty() || task_id.trim().is_empty() {
        return Err(HealthAcceptanceError::InvalidIdentity);
    }

    append(
        state,
        project_id,
        run_id,
        task_id,
        HEALTH_EVENT,
        outcome_payload(verdict),
    )?;

    match verdict {
        HealthVerdict::Block(violations) => Ok(HealthAcceptance::Blocked(violations.clone())),
        HealthVerdict::Accept => Ok(HealthAcceptance::Clear),
        HealthVerdict::Review(findings) => {
            let history = regression_history(state, project_id)?;
            let mut required = Vec::new();
            for finding in findings {
                let Some(path) = finding.path.as_deref() else {
                    continue;
                };
                let dimension = finding.dimension.as_str();
                let observed = history
                    .iter()
                    .filter(|(recorded_path, recorded_dimension)| {
                        recorded_path == path && recorded_dimension == dimension
                    })
                    .count();
                if observed < REPEATED_REGRESSION_THRESHOLD {
                    continue;
                }
                required.push(RefactoringTask {
                    task_id: refactoring_task_id(path, dimension),
                    target_path: path.to_owned(),
                    dimension: dimension.to_owned(),
                    observed_regressions: observed,
                    triggering_task_id: task_id.to_owned(),
                });
            }

            if required.is_empty() {
                return Ok(HealthAcceptance::Recorded(findings.clone()));
            }
            for task in &required {
                append(
                    state,
                    project_id,
                    run_id,
                    task_id,
                    REFACTORING_EVENT,
                    json!({
                        "refactoring_task_id": task.task_id,
                        "target_path": task.target_path,
                        "dimension": task.dimension,
                        "observed_regressions": task.observed_regressions,
                    }),
                )?;
            }
            Ok(HealthAcceptance::RefactoringRequired(required))
        }
    }
}

/// Every recorded regression in the project, as `(path, dimension)` pairs.
///
/// The history is read from the journal rather than kept in memory so a
/// restarted runtime counts the same regressions the previous one did.
fn regression_history(
    state: &DurableState,
    project_id: &str,
) -> Result<Vec<(String, String)>, HealthAcceptanceError> {
    let events = state
        .events(project_id)
        .map_err(|error| HealthAcceptanceError::Storage(error.to_string()))?;
    let mut history = Vec::new();
    for event in events
        .iter()
        .filter(|event| event.event_type == HEALTH_EVENT)
    {
        let Some(payload) = parse_payload(event) else {
            continue;
        };
        let Some(findings) = payload.get("findings").and_then(Value::as_array) else {
            continue;
        };
        for finding in findings {
            let (Some(path), Some(dimension)) = (
                finding.get("path").and_then(Value::as_str),
                finding.get("dimension").and_then(Value::as_str),
            ) else {
                continue;
            };
            history.push((path.to_owned(), dimension.to_owned()));
        }
    }
    Ok(history)
}

fn parse_payload(event: &StoredEvent) -> Option<Value> {
    event
        .payload_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
}

fn outcome_payload(verdict: &HealthVerdict) -> Value {
    match verdict {
        HealthVerdict::Accept => json!({ "verdict": "accept", "findings": [] }),
        HealthVerdict::Review(findings) => json!({
            "verdict": "review",
            "findings": findings.iter().map(finding_json).collect::<Vec<_>>(),
        }),
        HealthVerdict::Block(violations) => json!({
            "verdict": "block",
            "findings": [],
            "boundary_violations": violations
                .iter()
                .map(|violation| json!({
                    "from_path": violation.from_path,
                    "to_path": violation.to_path,
                    "from_layer": violation.from_layer,
                    "to_layer": violation.to_layer,
                }))
                .collect::<Vec<_>>(),
        }),
    }
}

fn finding_json(finding: &Finding) -> Value {
    json!({
        "dimension": finding.dimension.as_str(),
        "path": finding.path,
        "before": finding.before,
        "after": finding.after,
        // The tier travels with the finding so a syntax-derived number is never
        // read later as if a compiler had produced it.
        "capability_tier": finding.tier.as_u8(),
    })
}

fn refactoring_task_id(path: &str, dimension: &str) -> String {
    format!("refactor:{dimension}:{path}")
}

fn append(
    state: &mut DurableState,
    project_id: &str,
    run_id: Option<&str>,
    task_id: &str,
    event_type: &str,
    payload: Value,
) -> Result<StoredEvent, HealthAcceptanceError> {
    let mut event = NewEvent::new(project_id, event_type);
    event.run_id = run_id.map(str::to_owned);
    event.task_id = Some(task_id.to_owned());
    event.payload = EventPayload::Inline(payload);
    event.correlation_id = Some(task_id.to_owned());
    state
        .append_event(event)
        .map_err(|error| HealthAcceptanceError::Storage(error.to_string()))
}

/// Failures this module can report.
#[derive(Debug, Eq, PartialEq)]
pub enum HealthAcceptanceError {
    /// The project or task identity was empty.
    InvalidIdentity,
    /// The durable journal could not be read or appended to.
    Storage(String),
}

impl std::fmt::Display for HealthAcceptanceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentity => {
                formatter.write_str("architecture health needs a project and task identity")
            }
            Self::Storage(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for HealthAcceptanceError {}

#[cfg(test)]
mod tests {
    use aer_health::{HealthDimension, HealthPolicy, HealthVerdict, evaluate};
    use aer_repo::CapabilityTier;

    use aer_storage::DurableState;
    use ulid::Ulid;

    use super::*;

    /// A durable store in a directory that disappears with the test.
    struct TemporaryState {
        path: std::path::PathBuf,
    }

    impl TemporaryState {
        fn open(label: &str) -> (Self, DurableState) {
            let path =
                std::env::temp_dir().join(format!("aer-health-{label}-{}", Ulid::generate()));
            std::fs::create_dir_all(&path).expect("create test directory");
            let state = DurableState::open(path.join("durable")).expect("store");
            (Self { path }, state)
        }
    }

    impl Drop for TemporaryState {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn temporary_state(label: &str) -> (TemporaryState, DurableState) {
        TemporaryState::open(label)
    }

    fn finding(path: &str, before: u32, after: u32) -> Finding {
        Finding {
            dimension: HealthDimension::LargestUnitLines,
            path: Some(path.to_owned()),
            before,
            after,
            tier: CapabilityTier::Tier1Syntax,
        }
    }

    fn violation() -> BoundaryViolation {
        BoundaryViolation {
            from_path: "crates/aer-exec".to_owned(),
            to_path: "crates/aer-core".to_owned(),
            from_layer: "infrastructure".to_owned(),
            to_layer: "application".to_owned(),
        }
    }

    #[test]
    fn a_clean_outcome_is_still_journalled_so_the_series_exists() {
        let (_root, mut state) = temporary_state("health-clear");
        let acceptance = record_health_outcome(
            &mut state,
            "project",
            None,
            "task-1",
            &HealthVerdict::Accept,
        )
        .expect("record");
        assert_eq!(acceptance, HealthAcceptance::Clear);
        assert!(acceptance.permits_acceptance());
        let events = state.events("project").expect("events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == HEALTH_EVENT)
                .count(),
            1
        );
    }

    #[test]
    fn a_crossed_boundary_refuses_acceptance() {
        let (_root, mut state) = temporary_state("health-block");
        let acceptance = record_health_outcome(
            &mut state,
            "project",
            None,
            "task-1",
            &HealthVerdict::Block(vec![violation()]),
        )
        .expect("record");
        assert!(!acceptance.permits_acceptance());
        let HealthAcceptance::Blocked(violations) = acceptance else {
            panic!("a crossed boundary must block");
        };
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn one_regression_is_recorded_without_demanding_a_refactor() {
        let (_root, mut state) = temporary_state("health-single");
        let acceptance = record_health_outcome(
            &mut state,
            "project",
            None,
            "task-1",
            &HealthVerdict::Review(vec![finding("src/engine.rs", 100, 200)]),
        )
        .expect("record");
        assert!(matches!(acceptance, HealthAcceptance::Recorded(_)));
    }

    #[test]
    fn a_hotspot_that_keeps_regressing_produces_a_refactoring_task() {
        let (_root, mut state) = temporary_state("health-repeat");
        let verdict = HealthVerdict::Review(vec![finding("src/engine.rs", 100, 200)]);
        for index in 0..REPEATED_REGRESSION_THRESHOLD {
            let acceptance = record_health_outcome(
                &mut state,
                "project",
                None,
                &format!("task-{index}"),
                &verdict,
            )
            .expect("record");
            if index + 1 < REPEATED_REGRESSION_THRESHOLD {
                assert!(
                    matches!(acceptance, HealthAcceptance::Recorded(_)),
                    "refactor demanded after {} regression(s)",
                    index + 1
                );
            } else {
                let HealthAcceptance::RefactoringRequired(tasks) = acceptance else {
                    panic!("repeated regression must create a refactoring task");
                };
                assert_eq!(tasks.len(), 1);
                assert_eq!(tasks[0].target_path, "src/engine.rs");
                assert_eq!(tasks[0].observed_regressions, REPEATED_REGRESSION_THRESHOLD);
                assert_eq!(
                    tasks[0].task_id, "refactor:largest-unit-lines:src/engine.rs",
                    "a hotspot must keep pointing at one task rather than spawning new ones"
                );
            }
        }
        let events = state.events("project").expect("events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == REFACTORING_EVENT)
                .count(),
            1
        );
    }

    #[test]
    fn regressions_in_different_files_do_not_add_up_to_one_hotspot() {
        let (_root, mut state) = temporary_state("health-spread");
        for index in 0..(REPEATED_REGRESSION_THRESHOLD + 1) {
            let verdict = HealthVerdict::Review(vec![finding(&format!("src/f{index}.rs"), 10, 90)]);
            let acceptance = record_health_outcome(
                &mut state,
                "project",
                None,
                &format!("task-{index}"),
                &verdict,
            )
            .expect("record");
            assert!(
                matches!(acceptance, HealthAcceptance::Recorded(_)),
                "unrelated files must not accumulate into a refactor demand"
            );
        }
    }

    #[test]
    fn the_recorded_outcome_keeps_the_capability_tier_of_its_measurement() {
        let (_root, mut state) = temporary_state("health-tier");
        record_health_outcome(
            &mut state,
            "project",
            None,
            "task-1",
            &HealthVerdict::Review(vec![finding("src/engine.rs", 100, 200)]),
        )
        .expect("record");
        let events = state.events("project").expect("events");
        let payload = events
            .iter()
            .find(|event| event.event_type == HEALTH_EVENT)
            .and_then(super::parse_payload)
            .expect("payload");
        let tier = payload["findings"][0]["capability_tier"].as_u64();
        assert_eq!(tier, Some(u64::from(CapabilityTier::Tier1Syntax.as_u8())));
    }

    #[test]
    fn an_empty_identity_is_refused_before_anything_is_written() {
        let (_root, mut state) = temporary_state("health-identity");
        assert_eq!(
            record_health_outcome(&mut state, " ", None, "task-1", &HealthVerdict::Accept),
            Err(HealthAcceptanceError::InvalidIdentity)
        );
    }

    #[test]
    fn an_accepting_policy_and_an_accepting_verdict_agree() {
        // The acceptance path must read the same verdict the gate produced, so
        // the two are wired here rather than reimplemented.
        let verdict = evaluate(&[], &[], &[], HealthPolicy::default(), 0);
        let (_root, mut state) = temporary_state("health-wired");
        let acceptance =
            record_health_outcome(&mut state, "project", None, "task-1", &verdict).expect("record");
        assert_eq!(acceptance, HealthAcceptance::Clear);
    }
}
