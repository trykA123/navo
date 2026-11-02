use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Summary {
    pub count: usize,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

impl Summary {
    #[must_use]
    pub fn of(values: impl IntoIterator<Item = f64>) -> Option<Self> {
        let mut sorted: Vec<f64> = values
            .into_iter()
            .filter(|value| value.is_finite())
            .collect();
        sorted.sort_by(f64::total_cmp);
        let max = *sorted.last()?;
        Some(Self {
            count: sorted.len(),
            p50: rank(&sorted, 50),
            p95: rank(&sorted, 95),
            p99: rank(&sorted, 99),
            max,
        })
    }
}

fn rank(sorted: &[f64], percent: usize) -> f64 {
    let index = (sorted.len() * percent).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}
