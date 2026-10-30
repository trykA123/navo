use std::path::Path;

use serde::Serialize;

use crate::stats::Summary;

#[derive(Debug, Serialize)]
pub struct Timing {
    pub cold_ms: f64,
    pub median_ms: f64,
    pub samples: Vec<f64>,
}

impl Timing {
    pub fn of(samples: Vec<f64>) -> Self {
        let cold_ms = samples.first().copied().unwrap_or(0.0);
        let warm = samples.get(1..).unwrap_or(&[]);
        let median_ms = Summary::of(warm)
            .or_else(|| Summary::of(&samples))
            .map_or(0.0, |summary| summary.median);
        Self {
            cold_ms,
            median_ms,
            samples,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ModeTiming {
    pub mode: &'static str,
    pub timing: Timing,
}

#[derive(Debug, Serialize)]
pub struct TextRow {
    pub name: String,
    pub old_bytes: u64,
    pub new_bytes: u64,
    pub old_lines: u64,
    pub new_lines: u64,
    pub candidates_old: usize,
    pub candidates_new: usize,
    pub unique_matches: usize,
    pub anchors: usize,
    pub sections: usize,
    pub reanchored: u64,
    pub coarse: u64,
    pub changed_lines: u64,
    pub hunks: u64,
    pub scan_old: Vec<ModeTiming>,
    pub scan_new: Vec<ModeTiming>,
    pub anchors_timing: Timing,
    pub sections_timing: Timing,
    pub total_cold_ms: f64,
    pub total_ms: f64,
    pub provisional: Timing,
    pub coarse_256: u64,
    pub coarse_1024: u64,
    pub tracked_peak_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct ByteRow {
    pub name: String,
    pub alignment: &'static str,
    pub old_bytes: u64,
    pub new_bytes: u64,
    pub regions: u64,
    pub changed_bytes: u64,
    pub inserted_bytes: u64,
    pub refined_bytes: u64,
    pub first_difference: Option<u64>,
    pub timing: Timing,
    pub tracked_peak_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct SetRow {
    pub set: &'static str,
    pub pairs: u64,
    pub equal_checks: u64,
    pub first_difference: Option<u64>,
    pub buffered: Timing,
    pub mapped: Timing,
    pub mapped_peak_bytes: usize,
}

#[derive(Debug, Serialize)]
pub struct QualityRow {
    pub name: String,
    pub anchored: u64,
    pub histogram: u64,
    pub delta: u64,
    pub ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct Memory {
    pub tracked_peak_bytes: usize,
    pub peak_rss_mib: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StopReport {
    pub quick: bool,
    pub log_warm_ms: f64,
    pub log_target_ms: f64,
    pub log_over_target: bool,
    pub tracked_peak_mib: f64,
    pub peak_rss_mib: Option<f64>,
    pub tracked_over_512: bool,
    pub quality_pairs: usize,
    pub quality_mismatches: usize,
    pub quality_over_5pct: bool,
    pub total_seconds: f64,
    pub run_over_20_min: bool,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub quick: bool,
    pub huge: bool,
    pub runs: usize,
    pub text: Vec<TextRow>,
    pub binaries: Vec<ByteRow>,
    pub folder: Vec<SetRow>,
    pub quality: Vec<QualityRow>,
    pub memory: Memory,
    pub stop: StopReport,
    pub notes: Vec<String>,
}

pub fn render(report: &Report) -> String {
    let mut output = format!(
        "Bench-large: informational only. Times in milliseconds. {} runs; the first (cold) is reported apart; median is over the remaining three. Corpus: {}.\n\n",
        report.runs,
        if report.quick {
            "256 MiB quick"
        } else {
            "1 GiB full"
        }
    );
    render_text(report, &mut output);
    render_binaries(report, &mut output);
    render_folder(report, &mut output);
    render_quality(report, &mut output);
    render_stop(report, &mut output);
    for note in &report.notes {
        output.push_str(&format!("- {note}\n"));
    }
    output
}

fn render_text(report: &Report, output: &mut String) {
    output.push_str("## text\n\n");
    for row in &report.text {
        output.push_str(&format!(
            "### {}\n\n{} / {} bytes, {} / {} lines. {} / {} candidates, {} unique matches, {} anchors, {} sections ({} re-anchored, {} coarse). {} changed lines in {} hunks.\n\n| Stage | Cold | Median warm |\n| --- | ---: | ---: |\n",
            row.name,
            row.old_bytes,
            row.new_bytes,
            row.old_lines,
            row.new_lines,
            row.candidates_old,
            row.candidates_new,
            row.unique_matches,
            row.anchors,
            row.sections,
            row.reanchored,
            row.coarse,
            row.changed_lines,
            row.hunks
        ));
        for side in [("old", &row.scan_old), ("new", &row.scan_new)] {
            for mode in side.1 {
                output.push_str(&format!(
                    "| scan {} {} | {:.3} | {:.3} |\n",
                    side.0, mode.mode, mode.timing.cold_ms, mode.timing.median_ms
                ));
            }
        }
        output.push_str(&format!(
            "| anchors | {:.3} | {:.3} |\n| sections (Histogram) | {:.3} | {:.3} |\n| provisional first 4 MiB | {:.3} | {:.3} |\n| total (scan + anchors + sections) | {:.3} | {:.3} |\n\n",
            row.anchors_timing.cold_ms,
            row.anchors_timing.median_ms,
            row.sections_timing.cold_ms,
            row.sections_timing.median_ms,
            row.provisional.cold_ms,
            row.provisional.median_ms,
            row.total_cold_ms,
            row.total_ms
        ));
        output.push_str(&format!(
            "Coarse blocks: {} at the 256 MiB budget, {} at the 1 GiB budget. Tracked working memory: {:.1} MiB.\n\n",
            row.coarse_256,
            row.coarse_1024,
            mib(row.tracked_peak_bytes)
        ));
    }
}

fn render_binaries(report: &Report, output: &mut String) {
    output.push_str("## binaries\n\n| Pair | Alignment | Regions | Changed bytes | Inserted bytes | Refined bytes | First difference | Cold | Median warm |\n| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for row in &report.binaries {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {:.3} | {:.3} |\n",
            row.name,
            row.alignment,
            row.regions,
            row.changed_bytes,
            row.inserted_bytes,
            row.refined_bytes,
            row.first_difference
                .map_or_else(|| "none".to_owned(), |offset| offset.to_string()),
            row.timing.cold_ms,
            row.timing.median_ms
        ));
    }
    output.push('\n');
}

fn render_folder(report: &Report, output: &mut String) {
    output.push_str("## folder content check\n\n| Set | Pairs | Equal checks | First difference | Buffered cold | Buffered median | Mapped cold | Mapped median |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for row in &report.folder {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} |\n",
            row.set,
            row.pairs,
            row.equal_checks,
            row.first_difference
                .map_or_else(|| "none".to_owned(), |offset| offset.to_string()),
            row.buffered.cold_ms,
            row.buffered.median_ms,
            row.mapped.cold_ms,
            row.mapped.median_ms
        ));
    }
    output.push('\n');
}

fn render_quality(report: &Report, output: &mut String) {
    output.push_str("## quality (S2 200,000-line files)\n\n| Pair | Anchored changed lines | Whole-file Histogram | Delta | Ratio |\n| --- | ---: | ---: | ---: | ---: |\n");
    for row in &report.quality {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {:.2} % |\n",
            row.name,
            row.anchored,
            row.histogram,
            row.delta,
            row.ratio * 100.0
        ));
    }
    output.push('\n');
}

fn render_stop(report: &Report, output: &mut String) {
    let stop = &report.stop;
    output.push_str(&format!(
        "## stop conditions\n\n- 1 GiB log pair warm: {} ms against {} ms{}: **{}**.\n- Tracked working memory at the default budget: {:.1} MiB (limit 512, peak RSS {}): **{}**.\n- Changed-line count against Histogram: {} of {} pairs over 5 %: **{}**.\n- Run time: {:.1} s over 20 minutes: **{}**.\n\n",
        trim(stop.log_warm_ms),
        trim(stop.log_target_ms),
        if stop.quick { " (quick, target scaled to one quarter)" } else { "" },
        over(stop.log_over_target),
        stop.tracked_peak_mib,
        stop.peak_rss_mib
            .map_or_else(|| "n/a".to_owned(), |rss| format!("{rss:.1} MiB")),
        over(stop.tracked_over_512),
        stop.quality_mismatches,
        stop.quality_pairs,
        over(stop.quality_over_5pct),
        stop.total_seconds,
        over(stop.run_over_20_min)
    ));
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn trim(value: f64) -> String {
    format!("{value:.1}")
}

fn over(value: bool) -> &'static str {
    if value { "OVER" } else { "no" }
}

pub fn write_json(report: &Report, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(report).map_err(|error| error.to_string())?;
    std::fs::write(path, format!("{json}\n"))
        .map_err(|error| format!("{}: {error}", path.display()))
}
