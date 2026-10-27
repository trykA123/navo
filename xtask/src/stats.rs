#[derive(Debug, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub mean: f64,
    pub sd: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub outliers: Vec<usize>,
}

impl Summary {
    pub fn of(samples: &[f64]) -> Option<Self> {
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let min = *sorted.first()?;
        let max = *sorted.last()?;
        let count = sorted.len();
        let mean = samples.iter().sum::<f64>() / count as f64;
        let sd = if count == 1 {
            0.0
        } else {
            (samples
                .iter()
                .map(|value| (value - mean).powi(2))
                .sum::<f64>()
                / (count - 1) as f64)
                .sqrt()
        };
        let median = middle(&sorted);
        let mut deviations: Vec<_> = samples.iter().map(|value| (value - median).abs()).collect();
        deviations.sort_by(f64::total_cmp);
        let mad = middle(&deviations);
        let outliers = samples
            .iter()
            .enumerate()
            .filter(|(_, value)| mad > 0.0 && (0.6745 * (*value - median) / mad).abs() > 3.5)
            .map(|(index, _)| index)
            .collect();
        Some(Self {
            count,
            mean,
            sd,
            median,
            min,
            max,
            outliers,
        })
    }
}

fn middle(sorted: &[f64]) -> f64 {
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

#[cfg(test)]
mod tests {
    use super::Summary;

    #[test]
    fn known_samples_have_sample_statistics() {
        let summary = Summary::of(&[9.0, 1.0, 5.0, 3.0, 7.0]).unwrap();
        assert_eq!(summary.count, 5);
        assert_eq!(summary.mean, 5.0);
        assert!((summary.sd - 10.0_f64.sqrt()).abs() < 1e-12);
        assert_eq!(summary.median, 5.0);
        assert_eq!(summary.min, 1.0);
        assert_eq!(summary.max, 9.0);
        assert!(summary.outliers.is_empty());
    }

    #[test]
    fn outliers_keep_original_indices_in_both_tails() {
        for extreme in [-100.0, 100.0] {
            let summary = Summary::of(&[10.0, extreme, 11.0, 9.0, 12.0]).unwrap();
            assert_eq!(summary.outliers, vec![1]);
        }
    }

    #[test]
    fn two_samples_average_the_middle_pair() {
        let summary = Summary::of(&[5.0, 1.0]).unwrap();
        assert_eq!(summary.count, 2);
        assert_eq!(summary.mean, 3.0);
        assert!((summary.sd - 8.0_f64.sqrt()).abs() < 1e-12);
        assert_eq!(summary.median, 3.0);
        assert!(summary.outliers.is_empty());
    }

    #[test]
    fn one_sample_has_zero_standard_deviation() {
        let summary = Summary::of(&[7.0]).unwrap();
        assert_eq!(summary.count, 1);
        assert_eq!(summary.mean, 7.0);
        assert_eq!(summary.sd, 0.0);
        assert_eq!(summary.median, 7.0);
        assert_eq!(summary.min, 7.0);
        assert_eq!(summary.max, 7.0);
        assert!(summary.outliers.is_empty());
    }

    #[test]
    fn zero_median_absolute_deviation_has_no_outliers() {
        let summary = Summary::of(&[1.0, 1.0, 1.0, 100.0]).unwrap();
        assert!(summary.outliers.is_empty());
    }

    #[test]
    fn empty_input_has_no_summary() {
        assert_eq!(Summary::of(&[]), None);
    }
}
