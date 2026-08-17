//! Deterministic descriptive statistics for benchmark samples.
//!
//! Every figure a benchmark publishes is computed here, from the raw sample
//! vector, with no hidden filtering. The bootstrap uses a fixed-seed generator
//! so a published interval can be recomputed exactly rather than approximately.

use serde_json::{Value, json};

/// Number of bootstrap resamples. Fixed so intervals are reproducible.
pub const BOOTSTRAP_RESAMPLES: usize = 2000;

/// Seed for the bootstrap generator. Fixed for the same reason.
pub const BOOTSTRAP_SEED: u64 = 0x4145_525f_6265_6e63; // "AER_benc"

/// Descriptive statistics over a sample vector.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub min: Option<f64>,
    pub p25: Option<f64>,
    pub median: Option<f64>,
    pub p75: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub spread: Option<f64>,
}

impl Summary {
    /// Summarises a sample vector. An empty vector yields all-`None`.
    #[must_use]
    pub fn of(samples: &[f64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let min = sorted[0];
        let max = sorted[sorted.len() - 1];
        let sum: f64 = sorted.iter().sum();
        Self {
            count: sorted.len(),
            min: Some(min),
            p25: Some(percentile(&sorted, 0.25)),
            median: Some(percentile(&sorted, 0.50)),
            p75: Some(percentile(&sorted, 0.75)),
            max: Some(max),
            #[expect(
                clippy::cast_precision_loss,
                reason = "sample counts stay far below 2^53"
            )]
            mean: Some(sum / sorted.len() as f64),
            spread: Some(max - min),
        }
    }

    /// Machine-readable form. Absent statistics stay `null` rather than zero.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "count": self.count,
            "min": self.min,
            "p25": self.p25,
            "median": self.median,
            "p75": self.p75,
            "max": self.max,
            "mean": self.mean,
            "spread": self.spread,
        })
    }
}

/// Linear-interpolated percentile of an already sorted sample vector.
///
/// # Panics
///
/// Panics when `sorted` is empty.
#[must_use]
pub fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    assert!(!sorted.is_empty(), "percentile of an empty sample");
    if sorted.len() == 1 {
        return sorted[0];
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts stay far below 2^53"
    )]
    let position = quantile.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lower = position.floor();
    let upper = position.ceil();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "position is clamped into the index range above"
    )]
    let (lower_index, upper_index) = (lower as usize, upper as usize);
    if lower_index == upper_index {
        return sorted[lower_index];
    }
    let weight = position - lower;
    sorted[lower_index].mul_add(1.0 - weight, sorted[upper_index] * weight)
}

/// A percentile bootstrap confidence interval for the median.
///
/// Returns `None` for fewer than three samples: an interval over one or two
/// points states more confidence than the data carries.
#[must_use]
pub fn bootstrap_median_interval(samples: &[f64]) -> Option<(f64, f64)> {
    if samples.len() < 3 {
        return None;
    }
    let mut generator = SplitMix64::new(BOOTSTRAP_SEED);
    let mut medians = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
    let mut resample = vec![0.0; samples.len()];
    for _ in 0..BOOTSTRAP_RESAMPLES {
        for slot in &mut resample {
            let index = generator.below(samples.len());
            *slot = samples[index];
        }
        resample.sort_by(f64::total_cmp);
        medians.push(percentile(&resample, 0.50));
    }
    medians.sort_by(f64::total_cmp);
    Some((percentile(&medians, 0.025), percentile(&medians, 0.975)))
}

/// Paired comparison of two equal-length sample vectors.
///
/// `baseline` and `candidate` must be aligned: element `i` of each must come
/// from the same task, repetition and conditions. Pairs where either side is
/// missing are dropped, and the count of surviving pairs is reported.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PairedDelta {
    pub pairs: usize,
    pub absolute: Summary,
    pub percentage: Summary,
    pub interval: Option<(f64, f64)>,
}

impl PairedDelta {
    /// Computes `baseline - candidate` per pair, plus the relative reduction.
    ///
    /// A positive value therefore means the candidate used less of the metric.
    #[must_use]
    pub fn of(pairs: &[(f64, f64)]) -> Self {
        let absolute: Vec<f64> = pairs
            .iter()
            .map(|(base, candidate)| base - candidate)
            .collect();
        let percentage: Vec<f64> = pairs
            .iter()
            .filter(|(base, _)| *base != 0.0)
            .map(|(base, candidate)| (base - candidate) / base * 100.0)
            .collect();
        Self {
            pairs: pairs.len(),
            interval: bootstrap_median_interval(&absolute),
            absolute: Summary::of(&absolute),
            percentage: Summary::of(&percentage),
        }
    }

    /// Machine-readable form.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "pairs": self.pairs,
            "absolute": self.absolute.to_json(),
            "percentage": self.percentage.to_json(),
            "median_absolute_ci95": self.interval.map(|(low, high)| json!([low, high])),
        })
    }
}

/// Deterministic generator. Reproducibility matters more than statistical
/// pedigree here: the bootstrap only needs uniform indices, and a published
/// interval must be recomputable byte for byte.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    /// Uniform index below `bound`, rejecting the biased tail.
    fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "bound must be positive");
        let bound = bound as u64;
        let limit = u64::MAX - (u64::MAX % bound);
        loop {
            let value = self.next();
            if value < limit {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the modulus is below the original usize bound"
                )]
                return (value % bound) as usize;
            }
        }
    }
}

/// Ratio that stays `None` instead of dividing by zero.
///
/// A profile that verified nothing has no cost per verified success. Reporting
/// zero, or infinity, would both be lies.
#[must_use]
pub fn per_success(total: f64, successes: usize) -> Option<f64> {
    if successes == 0 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "success counts stay far below 2^53"
    )]
    Some(total / successes as f64)
}

/// Ratio that stays `None` for an empty denominator.
#[must_use]
pub fn mean_of(total: f64, count: usize) -> Option<f64> {
    if count == 0 {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts stay far below 2^53"
    )]
    Some(total / count as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_sample_has_no_statistics() {
        let summary = Summary::of(&[]);
        assert_eq!(summary.count, 0);
        assert_eq!(summary.median, None);
        assert_eq!(summary.mean, None);
        assert_eq!(summary.spread, None);
    }

    #[test]
    fn summary_matches_hand_computed_values() {
        let summary = Summary::of(&[4.0, 1.0, 3.0, 2.0]);
        assert_eq!(summary.count, 4);
        assert_eq!(summary.min, Some(1.0));
        assert_eq!(summary.max, Some(4.0));
        assert_eq!(summary.median, Some(2.5));
        assert_eq!(summary.p25, Some(1.75));
        assert_eq!(summary.p75, Some(3.25));
        assert_eq!(summary.mean, Some(2.5));
        assert_eq!(summary.spread, Some(3.0));
    }

    #[test]
    fn a_single_sample_has_zero_spread_and_is_its_own_median() {
        let summary = Summary::of(&[7.5]);
        assert_eq!(summary.median, Some(7.5));
        assert_eq!(summary.p25, Some(7.5));
        assert_eq!(summary.spread, Some(0.0));
    }

    #[test]
    fn paired_delta_is_baseline_minus_candidate() {
        let delta = PairedDelta::of(&[(100.0, 60.0), (200.0, 100.0)]);
        assert_eq!(delta.pairs, 2);
        assert_eq!(delta.absolute.median, Some(70.0));
        assert_eq!(delta.percentage.median, Some(45.0));
    }

    #[test]
    fn a_candidate_that_costs_more_reports_a_negative_delta() {
        let delta = PairedDelta::of(&[(10.0, 15.0)]);
        assert_eq!(delta.absolute.median, Some(-5.0));
        assert_eq!(delta.percentage.median, Some(-50.0));
    }

    #[test]
    fn a_zero_baseline_is_excluded_from_percentage_but_not_from_absolute() {
        let delta = PairedDelta::of(&[(0.0, 5.0), (10.0, 5.0)]);
        assert_eq!(delta.absolute.count, 2);
        assert_eq!(delta.percentage.count, 1);
        assert_eq!(delta.percentage.median, Some(50.0));
    }

    #[test]
    fn bootstrap_is_deterministic_and_brackets_the_median() {
        let samples = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let first = bootstrap_median_interval(&samples).expect("enough samples");
        let second = bootstrap_median_interval(&samples).expect("enough samples");
        assert!((first.0 - second.0).abs() < f64::EPSILON);
        assert!((first.1 - second.1).abs() < f64::EPSILON);
        assert!(
            first.0 <= 4.0 && first.1 >= 4.0,
            "interval {first:?} must contain the median"
        );
    }

    #[test]
    fn bootstrap_refuses_to_speak_for_two_samples() {
        assert_eq!(bootstrap_median_interval(&[1.0, 2.0]), None);
    }

    #[test]
    fn per_success_refuses_to_divide_by_zero() {
        assert_eq!(per_success(1.0, 0), None);
        assert_eq!(per_success(1.0, 4), Some(0.25));
        assert_eq!(mean_of(1.0, 0), None);
    }

    #[test]
    fn generator_indices_stay_in_range_and_cover_it() {
        let mut generator = SplitMix64::new(BOOTSTRAP_SEED);
        let mut seen = [false; 5];
        for _ in 0..500 {
            let index = generator.below(5);
            assert!(index < 5);
            seen[index] = true;
        }
        assert!(seen.iter().all(|hit| *hit), "every index must be reachable");
    }
}
