//! Stable interface for deterministic and model-backed benchmark fixtures.

use std::collections::BTreeMap;

/// Architecture-defined evaluation families that may own fixtures.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BenchmarkFamily {
    Intent,
    Research,
    Context,
    Provider,
    Router,
    Handoff,
    Resource,
    Proof,
    Domain,
    Evolution,
    Migration,
    SupplyChain,
    Integrity,
}

/// Normalized result returned by one benchmark fixture.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchmarkObservation {
    pub passed: bool,
    pub metrics: BTreeMap<String, f64>,
    pub notes: Vec<String>,
}

impl BenchmarkObservation {
    /// Constructs an observation with no metrics or notes yet.
    #[must_use]
    pub fn new(passed: bool) -> Self {
        Self {
            passed,
            metrics: BTreeMap::new(),
            notes: Vec::new(),
        }
    }
}

/// Minimal fixture ABI used by the evaluation harness.
///
/// A fixture supplies stable identity and family metadata, then returns an
/// observation. The interface intentionally does not prescribe a model provider,
/// executor, or persistence layer in Phase 0.
pub trait BenchmarkFixture {
    fn id(&self) -> &str;
    fn family(&self) -> BenchmarkFamily;
    fn evaluate(&self) -> BenchmarkObservation;
}

#[cfg(test)]
mod tests {
    use super::{BenchmarkFamily, BenchmarkFixture, BenchmarkObservation};

    struct DeterministicFixture;

    impl BenchmarkFixture for DeterministicFixture {
        fn id(&self) -> &str {
            "phase0.contract-smoke"
        }

        fn family(&self) -> BenchmarkFamily {
            BenchmarkFamily::Integrity
        }

        fn evaluate(&self) -> BenchmarkObservation {
            BenchmarkObservation::new(true)
        }
    }

    #[test]
    fn fixture_interface_carries_stable_identity_family_and_outcome() {
        let fixture = DeterministicFixture;
        assert_eq!(fixture.id(), "phase0.contract-smoke");
        assert_eq!(fixture.family(), BenchmarkFamily::Integrity);
        assert!(fixture.evaluate().passed);
    }
}
