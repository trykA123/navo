/// Fewer samples than this make p95 equal to the maximum, and the report says "max".
pub const P95_MIN_SAMPLES: usize = 20;
/// D93: a measure is noisy when its interquartile range exceeds this share of its median.
pub const NOISE_RATIO: f64 = 0.15;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub median: f64,
    pub p95: f64,
    /// True when `p95` is the maximum because there are fewer than `P95_MIN_SAMPLES`.
    pub p95_is_max: bool,
    pub iqr: f64,
    pub noisy: bool,
}

/// Summarises finite samples; `None` when empty or any sample is not finite.
pub fn summarize(samples: &[f64]) -> Option<Summary> {
    if samples.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = middle(&sorted)?;
    let iqr = iqr_sorted(&sorted)?;
    Some(Summary {
        count: sorted.len(),
        median,
        p95: nearest_rank(&sorted, 95)?,
        p95_is_max: sorted.len() < P95_MIN_SAMPLES,
        iqr,
        noisy: iqr > NOISE_RATIO * median.abs(),
    })
}

pub fn median(samples: &[f64]) -> Option<f64> {
    summarize(samples).map(|summary| summary.median)
}

/// Nearest rank: `sorted[ceil(p / 100 * n) - 1]`.
pub fn p95(samples: &[f64]) -> Option<f64> {
    summarize(samples).map(|summary| summary.p95)
}

/// Interquartile range from the medians of the lower and upper halves (middle excluded when odd).
pub fn iqr(samples: &[f64]) -> Option<f64> {
    summarize(samples).map(|summary| summary.iqr)
}

fn middle(sorted: &[f64]) -> Option<f64> {
    let half = sorted.len() / 2;
    if sorted.is_empty() {
        None
    } else if sorted.len().is_multiple_of(2) {
        Some((sorted[half - 1] + sorted[half]) / 2.0)
    } else {
        Some(sorted[half])
    }
}

fn iqr_sorted(sorted: &[f64]) -> Option<f64> {
    let half = sorted.len() / 2;
    if half == 0 {
        return Some(0.0);
    }
    let lower = middle(&sorted[..half])?;
    let upper = middle(&sorted[sorted.len() - half..])?;
    Some(upper - lower)
}

fn nearest_rank(sorted: &[f64], percent: usize) -> Option<f64> {
    let rank = (sorted.len() * percent).div_ceil(100).max(1);
    sorted.get(rank - 1).copied()
}
