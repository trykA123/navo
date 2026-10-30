mod anchor;
mod bytes;
mod content;
#[path = "gen.rs"]
mod generate;
mod measure;
mod options;
mod patterns;
mod quality;
mod report;
mod scan;
mod text;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use measure::{
    FULL_TEXT, RUNS, SMALL_BYTES, SMALL_PAIRS, Sizes, measure_corpus, quality_rows, sizes,
};
use options::{Options, parse};
use report::{ByteRow, QualityRow, Report, SetRow, StopReport, TextRow};

const WINDOWS_TARGET_MS: f64 = 15_000.0;
const RUN_LIMIT: Duration = Duration::from_secs(20 * 60);
const MEMORY_LIMIT_BYTES: usize = 512 * 1024 * 1024;
const QUALITY_LIMIT: f64 = 0.05;

pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("bench-large: {error}");
            eprintln!("usage: cargo xtask bench-large [--quick] [--huge] [--json <path>]");
            return ExitCode::from(2);
        }
    };
    match benchmark(&options) {
        Ok(report) => {
            print!("{}", report::render(&report));
            if let Some(path) = &options.json
                && let Err(error) = report::write_json(&report, path)
            {
                eprintln!("bench-large: {error}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("bench-large: {error}");
            ExitCode::FAILURE
        }
    }
}

fn benchmark(options: &Options) -> Result<Report, String> {
    reset_peak_rss();
    let started = Instant::now();
    let corpus = generate::Corpus::prepare(PathBuf::from("target/bench-large"))?;
    let sizes = sizes(options);
    let (text, binaries, folder) = measure_corpus(options, &corpus, &sizes)?;
    let quality = quality_rows()?;
    let tracked_peak = tracked_peak(&text, &binaries, &folder);
    let budget_peak = text
        .iter()
        .map(|row| row.tracked_peak_bytes)
        .chain(binaries.iter().map(|row| row.tracked_peak_bytes))
        .max()
        .unwrap_or(0);
    let stop = build_stop(RunFacts {
        options,
        sizes: &sizes,
        text: &text,
        quality: &quality,
        budget_peak,
        started,
    });
    let notes = notes(options, sizes.text, sizes.content);
    Ok(Report {
        quick: options.quick,
        huge: options.huge,
        runs: RUNS,
        text,
        binaries,
        folder,
        quality,
        memory: report::Memory {
            tracked_peak_bytes: tracked_peak,
            peak_rss_mib: peak_rss_mib(),
        },
        stop,
        notes,
    })
}

fn tracked_peak(text: &[TextRow], binaries: &[ByteRow], folder: &[SetRow]) -> usize {
    text.iter()
        .map(|row| row.tracked_peak_bytes)
        .chain(binaries.iter().map(|row| row.tracked_peak_bytes))
        .chain(folder.iter().map(|row| row.mapped_peak_bytes))
        .max()
        .unwrap_or(0)
}

struct RunFacts<'a> {
    options: &'a Options,
    sizes: &'a Sizes,
    text: &'a [TextRow],
    quality: &'a [QualityRow],
    budget_peak: usize,
    started: Instant,
}

fn build_stop(facts: RunFacts) -> StopReport {
    let log_warm_ms = facts.text.first().map_or(0.0, |row| row.total_ms);
    let log_target_ms = WINDOWS_TARGET_MS * facts.sizes.text as f64 / FULL_TEXT as f64;
    let mismatches = facts
        .quality
        .iter()
        .filter(|row| row.ratio > QUALITY_LIMIT)
        .count();
    StopReport {
        quick: facts.options.quick,
        log_warm_ms,
        log_target_ms,
        log_over_target: log_warm_ms > log_target_ms,
        tracked_peak_mib: bytes_to_mib(facts.budget_peak),
        peak_rss_mib: peak_rss_mib(),
        tracked_over_512: facts.budget_peak > MEMORY_LIMIT_BYTES,
        quality_pairs: facts.quality.len(),
        quality_mismatches: mismatches,
        quality_over_5pct: mismatches as f64 > facts.quality.len() as f64 * QUALITY_LIMIT,
        total_seconds: facts.started.elapsed().as_secs_f64(),
        run_over_20_min: facts.started.elapsed() > RUN_LIMIT,
    }
}

fn bytes_to_mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
fn notes(options: &Options, text_size: u64, content_size: u64) -> Vec<String> {
    let mut notes = vec![
        format!(
            "Corpus sizes: text pairs {} MiB, binary pairs {} MiB, large content pairs {} MiB, {} small pairs of {} bytes. Generated from a fixed seed under target/bench-large and deleted after each scenario and at the end, also on error.",
            text_size / (1024 * 1024),
            text_size / (1024 * 1024),
            content_size / (1024 * 1024),
            SMALL_PAIRS,
            SMALL_BYTES
        ),
        "Line keys are xxh3-64 over the trimmed line, sampled at 1 of K = 32 by hash. Candidates are 16 bytes (hash and line); anchors are patience LIS over unique candidates on both sides.".to_owned(),
        "xxhash-rust =0.8.18 (released 2026-07-21) and fastcdc =4.0.1 (released 2026-04-26), both over two weeks old at 2026-10-30. memmap2 =0.9.11 is pinned; it is used only in xtask.".to_owned(),
        "Sections are coalesced to at least 2,048 lines per side, capped at 64 MiB or 1,000,000 lines per side, and re-anchored at K/8 then every line when over the cap.".to_owned(),
        "Byte content alignment uses FastCDC 2 KiB / 8 KiB / 64 KiB chunks with a BLAKE3 hash each, unique on both sides, then the same patience LIS; regions up to 64 KiB refine with similar's byte diff.".to_owned(),
        "Peak RSS is /proc/self/status VmHWM on Linux; Windows has no portable peak-RSS read in this harness.".to_owned(),
    ];
    if options.quick {
        notes.push(
            "Quick mode uses 256 MiB text pairs instead of 1 GiB. The 15 s Windows target is scaled linearly by file size to 3.75 s for this run.".to_owned(),
        );
    }
    if options.huge {
        notes.push("--huge runs only the 4 GiB log pair.".to_owned());
    }
    if !cfg!(windows) {
        notes.push(
            "The windows-latest stop condition is not measurable on this Linux host; the local number is a proxy.".to_owned(),
        );
    }
    notes
}

#[cfg(target_os = "linux")]
fn peak_rss_mib() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmHWM:") {
            let kib = value.split_whitespace().next()?.parse::<f64>().ok()?;
            return Some(kib / 1024.0);
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn peak_rss_mib() -> Option<f64> {
    None
}

#[cfg(target_os = "linux")]
fn reset_peak_rss() {
    if let Err(error) = std::fs::write("/proc/self/clear_refs", "5") {
        eprintln!("bench-large: cannot reset peak RSS: {error}");
    }
}

#[cfg(not(target_os = "linux"))]
fn reset_peak_rss() {}
