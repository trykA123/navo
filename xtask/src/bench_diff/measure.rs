use std::hint::black_box;
use std::time::{Duration, Instant};

use imara_diff::{Algorithm as ImaraAlgorithm, Diff as ImaraDiff, InternedInput};
use similar::{Algorithm as SimilarAlgorithm, TextDiff};

use super::corpus::Pair;
use super::pairing::{self, Subhunk};

pub(super) const INTRALINE_DEADLINE: Duration = Duration::from_millis(5);
pub(super) const SHORT_LINE: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LineAlgorithm {
    ImaraHistogram,
    ImaraMyers,
    SimilarMyers,
    SimilarPatience,
}

impl LineAlgorithm {
    pub(super) const ALL: [Self; 4] = [
        Self::ImaraHistogram,
        Self::ImaraMyers,
        Self::SimilarMyers,
        Self::SimilarPatience,
    ];

    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::ImaraHistogram => "imara-histogram",
            Self::ImaraMyers => "imara-myers",
            Self::SimilarMyers => "similar-myers",
            Self::SimilarPatience => "similar-patience",
        }
    }
}

pub(super) fn line_samples(algorithm: LineAlgorithm, pairs: &[Pair], runs: usize) -> Vec<f64> {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            for pair in pairs {
                line_diff(algorithm, &pair.old, &pair.new);
            }
            elapsed_ms(started)
        })
        .collect()
}

pub(super) fn intraline_samples(
    rows: &[(String, String)],
    runs: usize,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut words = Vec::with_capacity(runs);
    let mut chars = Vec::with_capacity(runs);
    let mut combined = Vec::with_capacity(runs);
    for _ in 0..runs {
        let word_ms = measure_intraline(rows, false);
        let char_ms = measure_intraline(rows, true);
        words.push(word_ms);
        chars.push(char_ms);
        combined.push(word_ms + char_ms);
    }
    (words, chars, combined)
}

pub(super) fn pairing_samples(subhunk: &Subhunk, runs: usize) -> Vec<f64> {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            black_box(pairing::pair_rows(subhunk, 0.6).len());
            elapsed_ms(started)
        })
        .collect()
}

pub(super) fn line_diff(algorithm: LineAlgorithm, old: &str, new: &str) {
    match algorithm {
        LineAlgorithm::ImaraHistogram => imara(old, new, ImaraAlgorithm::Histogram),
        LineAlgorithm::ImaraMyers => imara(old, new, ImaraAlgorithm::Myers),
        LineAlgorithm::SimilarMyers => similar(old, new, SimilarAlgorithm::Myers),
        LineAlgorithm::SimilarPatience => similar(old, new, SimilarAlgorithm::Patience),
    }
}

fn imara(old: &str, new: &str, algorithm: ImaraAlgorithm) {
    let input = InternedInput::new(old, new);
    let diff = ImaraDiff::compute(algorithm, &input);
    black_box(diff.count_additions() + diff.count_removals());
}

fn similar(old: &str, new: &str, algorithm: SimilarAlgorithm) {
    let mut config = TextDiff::configure();
    config.algorithm(algorithm);
    let diff = config.diff_lines(old, new);
    black_box(diff.ops().len());
}

fn measure_intraline(rows: &[(String, String)], chars: bool) -> f64 {
    let started = Instant::now();
    for (old, new) in rows {
        if chars && !short_pair(old, new) {
            continue;
        }
        let mut config = TextDiff::configure();
        config.algorithm(SimilarAlgorithm::Myers);
        config.timeout(INTRALINE_DEADLINE);
        let diff = if chars {
            config.diff_chars(old, new)
        } else {
            config.diff_words(old, new)
        };
        black_box(diff.ops().len());
    }
    elapsed_ms(started)
}

fn short_pair(old: &str, new: &str) -> bool {
    old.chars().count() <= SHORT_LINE && new.chars().count() <= SHORT_LINE
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}
