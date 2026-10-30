use std::path::Path;

use serde::Serialize;

use crate::stats::Summary;

#[derive(Debug, Serialize)]
pub(super) struct Timing {
    pub cold_ms: f64,
    pub median_ms: f64,
    pub sd_ms: f64,
    pub samples: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub(super) struct AlgorithmResult {
    pub algorithm: &'static str,
    pub cold_ms: f64,
    pub median_ms: f64,
    pub sd_ms: f64,
    pub samples: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub(super) struct Scenario {
    pub name: String,
    pub files: usize,
    pub lines: usize,
    pub algorithms: Vec<AlgorithmResult>,
}

#[derive(Debug, Serialize)]
pub(super) struct ThresholdRow {
    pub threshold: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct StopReport {
    pub imara_histogram_slower_than_similar_10k: bool,
    pub line_plus_intraline_ms: f64,
    pub line_plus_intraline_over_50_ms: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct Report {
    pub quick: bool,
    pub runs: usize,
    pub scenarios: Vec<Scenario>,
    pub intraline_words: Timing,
    pub intraline_chars: Timing,
    pub intraline_combined: Timing,
    pub pairing: Timing,
    pub thresholds: Vec<ThresholdRow>,
    pub best_threshold: f64,
    pub stop: StopReport,
    pub notes: Vec<String>,
}

pub(super) fn timing(samples: Vec<f64>) -> Timing {
    let cold_ms = samples.first().copied().unwrap_or(0.0);
    let summary = Summary::of(samples.get(1..).unwrap_or(&[]));
    Timing {
        cold_ms,
        median_ms: summary.as_ref().map_or(0.0, |summary| summary.median),
        sd_ms: summary.as_ref().map_or(0.0, |summary| summary.sd),
        samples,
    }
}

pub(super) fn algorithm(name: &'static str, samples: Vec<f64>) -> AlgorithmResult {
    let timing = timing(samples);
    AlgorithmResult {
        algorithm: name,
        cold_ms: timing.cold_ms,
        median_ms: timing.median_ms,
        sd_ms: timing.sd_ms,
        samples: timing.samples,
    }
}

pub(super) fn render(report: &Report) -> String {
    let mut output = format!(
        "Bench-diff: informational only. Times in milliseconds. {} runs; the first (cold) run is reported apart; median is over the remaining {} runs.\n\n",
        report.runs,
        report.runs.saturating_sub(1)
    );
    render_scenarios(report, &mut output);
    render_intraline(report, &mut output);
    render_thresholds(report, &mut output);
    output
}

fn render_scenarios(report: &Report, output: &mut String) {
    for scenario in &report.scenarios {
        output.push_str(&format!(
            "### {}\n\n{} file(s), {} line(s).\n\n| Algorithm | Cold | Median warm | σ warm |\n| --- | ---: | ---: | ---: |\n",
            scenario.name, scenario.files, scenario.lines
        ));
        for algorithm in &scenario.algorithms {
            output.push_str(&format!(
                "| {} | {:.3} | {:.3} | {:.3} |\n",
                algorithm.algorithm, algorithm.cold_ms, algorithm.median_ms, algorithm.sd_ms
            ));
        }
        output.push('\n');
    }
}

fn render_intraline(report: &Report, output: &mut String) {
    output.push_str(
        "### intraline\n\n| Rows | Cold | Median warm | σ warm |\n| --- | ---: | ---: | ---: |\n",
    );
    for (name, timing) in [
        ("words (60)", &report.intraline_words),
        ("chars (short lines)", &report.intraline_chars),
        ("combined", &report.intraline_combined),
    ] {
        output.push_str(&format!(
            "| {name} | {:.3} | {:.3} | {:.3} |\n",
            timing.cold_ms, timing.median_ms, timing.sd_ms
        ));
    }
    output.push_str(&format!(
        "\n### pairing\n\n200-line subhunk, 60 % threshold. Cold {:.3} ms; median {:.3} ms; σ {:.3} ms.\n\n",
        report.pairing.cold_ms, report.pairing.median_ms, report.pairing.sd_ms
    ));
}

fn render_thresholds(report: &Report, output: &mut String) {
    output.push_str("### pairing threshold\n\n| Threshold | Precision | Recall | F1 | TP | FP | FN |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for row in &report.thresholds {
        output.push_str(&format!(
            "| {:.0} % | {:.3} | {:.3} | {:.3} | {} | {} | {} |\n",
            row.threshold * 100.0,
            row.precision,
            row.recall,
            row.f1,
            row.true_positives,
            row.false_positives,
            row.false_negatives
        ));
    }
    output.push_str(&format!(
        "\nBest threshold: {:.0} %.\n\n### stop conditions\n\n- imara Histogram slower than similar on 10,000-line files: {}.\n- line diff + 60 intraline rows over 50 ms: {} ({} ms).\n\n",
        report.best_threshold * 100.0,
        yes_no(report.stop.imara_histogram_slower_than_similar_10k),
        yes_no(report.stop.line_plus_intraline_over_50_ms),
        report.stop.line_plus_intraline_ms
    ));
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
}

pub(super) fn write_json(report: &Report, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(report).map_err(|error| error.to_string())?;
    std::fs::write(path, format!("{json}\n"))
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
