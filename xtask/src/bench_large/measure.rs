use std::time::Instant;

use super::anchor;
use super::options::Options;
use super::report::{ByteRow, ModeTiming, QualityRow, SetRow, TextRow, Timing};
use super::{bytes, content, generate, patterns, quality, scan, text};

pub(super) const RUNS: usize = 4;
pub(super) const QUICK_TEXT: u64 = 256 * 1024 * 1024;
pub(super) const FULL_TEXT: u64 = 1024 * 1024 * 1024;
const QUICK_CONTENT: u64 = 16 * 1024 * 1024;
const FULL_CONTENT: u64 = 64 * 1024 * 1024;
const LARGE_PAIRS: u64 = 10;
pub(super) const SMALL_PAIRS: u64 = 10_000;
pub(super) const SMALL_BYTES: u64 = 4096;
const DEFAULT_BUDGET: u64 = 1024 * 1024 * 1024;
const COARSE_BUDGET: u64 = 256 * 1024 * 1024;
const PROVISIONAL_BYTES: u64 = 4 * 1024 * 1024;

pub(super) struct Sizes {
    pub text: u64,
    pub content: u64,
}

pub(super) fn sizes(options: &Options) -> Sizes {
    Sizes {
        text: if options.huge {
            4 * FULL_TEXT
        } else if options.quick {
            QUICK_TEXT
        } else {
            FULL_TEXT
        },
        content: if options.quick {
            QUICK_CONTENT
        } else {
            FULL_CONTENT
        },
    }
}

pub(super) type CorpusRows = (Vec<TextRow>, Vec<ByteRow>, Vec<SetRow>);

pub(super) fn measure_corpus(
    options: &Options,
    corpus: &generate::Corpus,
    sizes: &Sizes,
) -> Result<CorpusRows, String> {
    if options.huge {
        let text = vec![text_scenario(corpus, "log-4gib", sizes.text)?];
        return Ok((text, Vec::new(), Vec::new()));
    }
    let label = if options.quick { "256mib" } else { "1gib" };
    let text = vec![
        text_scenario(corpus, &format!("log-{label}"), sizes.text)?,
        text_scenario(corpus, &format!("log-worst-{label}"), sizes.text)?,
    ];
    let binaries = binary_scenarios(corpus, sizes.text)?;
    let folder = folder_scenarios(corpus, sizes.content)?;
    Ok((text, binaries, folder))
}

fn text_scenario(corpus: &generate::Corpus, name: &str, size: u64) -> Result<TextRow, String> {
    let dir = corpus.dir(name)?;
    let generated = generate::log_pair(corpus, name, size, name.contains("worst"))?;
    let measured = measure_text(&generated.pair)?;
    corpus.remove(&dir)?;
    let row = TextRow {
        name: name.to_owned(),
        old_bytes: generated.pair.old_bytes,
        new_bytes: generated.pair.new_bytes,
        old_lines: measured.old_lines,
        new_lines: measured.new_lines,
        candidates_old: measured.candidates_old,
        candidates_new: measured.candidates_new,
        unique_matches: measured.unique_matches,
        anchors: measured.anchors,
        sections: measured.sections,
        reanchored: measured.reanchored,
        coarse: measured.coarse,
        changed_lines: measured.changed_lines,
        hunks: measured.hunks,
        scan_old: measured.scan_old,
        scan_new: measured.scan_new,
        anchors_timing: measured.anchors_timing,
        sections_timing: measured.sections_timing,
        total_cold_ms: measured.total_cold_ms,
        total_ms: measured.total_ms,
        provisional: measured.provisional,
        coarse_256: measured.coarse_256,
        coarse_1024: measured.coarse_1024,
        tracked_peak_bytes: measured.tracked_peak,
    };
    Ok(row)
}

struct TextMeasure {
    old_lines: u64,
    new_lines: u64,
    candidates_old: usize,
    candidates_new: usize,
    unique_matches: usize,
    anchors: usize,
    sections: usize,
    reanchored: u64,
    coarse: u64,
    changed_lines: u64,
    hunks: u64,
    scan_old: Vec<ModeTiming>,
    scan_new: Vec<ModeTiming>,
    anchors_timing: Timing,
    sections_timing: Timing,
    total_cold_ms: f64,
    total_ms: f64,
    provisional: Timing,
    coarse_256: u64,
    coarse_1024: u64,
    tracked_peak: usize,
}

fn measure_text(pair: &generate::Pair) -> Result<TextMeasure, String> {
    let (scan_old, scanned_old) = measure_scan(&pair.old)?;
    let (scan_new, scanned_new) = measure_scan(&pair.new)?;
    let mut old_candidates = scanned_old.candidates.clone();
    let mut new_candidates = scanned_new.candidates.clone();
    let first_anchors = anchor::find(&mut old_candidates, &mut new_candidates);
    let anchor_count = first_anchors.anchors.len();
    let anchors = first_anchors.anchors.clone();
    let anchors_timing = measure_anchors(&scanned_old, &scanned_new)?;
    let old_file = rusty_fs::PagedFile::open(&pair.old)?;
    let new_file = rusty_fs::PagedFile::open(&pair.new)?;
    let sides = text_sides(&old_file, &new_file, &scanned_old, &scanned_new);
    let (sections_timing, (plan, diff)) = sample_sections(&sides, &anchors)?;
    let provisional = sample(|| text::provisional(&old_file, &new_file, PROVISIONAL_BYTES))
        .map(|(timing, ())| timing)?;
    let (total_cold_ms, total_ms) =
        total_timings(&scan_old, &scan_new, &anchors_timing, &sections_timing);
    let scan_memory = scanned_old.tracked_bytes.max(scanned_new.tracked_bytes);
    let anchor_memory =
        first_anchors.tracked_bytes + old_candidates.len() * 16 + new_candidates.len() * 16;
    let tracked_peak = scan_memory
        .max(anchor_memory)
        .max(plan.tracked_bytes + diff.tracked_bytes + scan_memory / 4);
    Ok(TextMeasure {
        old_lines: scanned_old.lines,
        new_lines: scanned_new.lines,
        candidates_old: scanned_old.candidates.len(),
        candidates_new: scanned_new.candidates.len(),
        unique_matches: first_anchors.unique_matches,
        anchors: anchor_count,
        sections: plan.sections.len(),
        reanchored: plan.reanchored,
        coarse: plan.coarse(),
        changed_lines: diff.changed_lines,
        hunks: diff.hunks,
        scan_old,
        scan_new,
        anchors_timing,
        sections_timing,
        total_cold_ms,
        total_ms,
        provisional,
        coarse_256: plan.coarse_at(COARSE_BUDGET),
        coarse_1024: plan.coarse_at(1024 * 1024 * 1024),
        tracked_peak,
    })
}

fn text_sides<'a>(
    old_file: &'a rusty_fs::PagedFile,
    new_file: &'a rusty_fs::PagedFile,
    old: &'a scan::Scanned,
    new: &'a scan::Scanned,
) -> text::Sides<'a> {
    text::Sides {
        old: text::Side {
            file: old_file,
            index: &old.index,
            lines: old.lines,
        },
        new: text::Side {
            file: new_file,
            index: &new.index,
            lines: new.lines,
        },
    }
}

fn sample_sections(
    sides: &text::Sides,
    anchors: &[anchor::Anchor],
) -> Result<(Timing, (text::Plan, text::DiffResult)), String> {
    sample(|| {
        let plan = text::plan(sides, anchors)?;
        let diff = text::diff(
            sides.old.file,
            sides.new.file,
            &plan.sections,
            DEFAULT_BUDGET,
        )?;
        Ok((plan, diff))
    })
}

fn measure_scan(path: &std::path::Path) -> Result<(Vec<ModeTiming>, scan::Scanned), String> {
    let mut timings = Vec::with_capacity(scan::IoMode::ALL.len());
    let mut primary = None;
    for mode in scan::IoMode::ALL {
        let mut samples = Vec::with_capacity(RUNS);
        for _ in 0..RUNS {
            let started = Instant::now();
            let scanned = scan::scan_path(path, mode)?;
            samples.push(elapsed_ms(started));
            if mode == scan::IoMode::Paged && primary.is_none() {
                primary = Some(scanned);
            }
        }
        timings.push(ModeTiming {
            mode: mode.name(),
            timing: Timing::of(samples),
        });
    }
    let primary = primary.ok_or("no primary scan")?;
    Ok((timings, primary))
}

fn measure_anchors(old: &scan::Scanned, new: &scan::Scanned) -> Result<Timing, String> {
    let timing = sample(|| {
        let mut old_candidates = old.candidates.clone();
        let mut new_candidates = new.candidates.clone();
        Ok(anchor::find(&mut old_candidates, &mut new_candidates))
    })
    .map(|(timing, _)| timing)?;
    Ok(timing)
}

fn total_timings(
    scan_old: &[ModeTiming],
    scan_new: &[ModeTiming],
    anchors: &Timing,
    sections: &Timing,
) -> (f64, f64) {
    let cold = scan_cold(scan_old) + scan_cold(scan_new) + anchors.cold_ms + sections.cold_ms;
    let median =
        scan_median(scan_old) + scan_median(scan_new) + anchors.median_ms + sections.median_ms;
    (cold, median)
}

fn scan_median(timings: &[ModeTiming]) -> f64 {
    paged_timing(timings).map_or(0.0, |timing| timing.median_ms)
}

fn scan_cold(timings: &[ModeTiming]) -> f64 {
    paged_timing(timings).map_or(0.0, |timing| timing.cold_ms)
}

fn paged_timing(timings: &[ModeTiming]) -> Option<&Timing> {
    timings
        .iter()
        .find(|timing| timing.mode == scan::IoMode::Paged.name())
        .map(|timing| &timing.timing)
}

fn binary_scenarios(corpus: &generate::Corpus, size: u64) -> Result<Vec<ByteRow>, String> {
    let patched_dir = corpus.dir("binary-patched")?;
    let patched = generate::binary_patched(corpus, "binary-patched", size)?;
    let (offset_timing, offset) =
        sample(|| bytes::by_offset(&patched.pair.old, &patched.pair.new))?;
    corpus.remove(&patched_dir)?;
    let inserted_dir = corpus.dir("binary-inserted")?;
    let inserted = generate::binary_inserted(corpus, "binary-inserted", size)?;
    let (content_timing, content) =
        sample(|| bytes::by_content(&inserted.pair.old, &inserted.pair.new))?;
    corpus.remove(&inserted_dir)?;
    Ok(vec![
        ByteRow {
            name: "patched 1,000 ranges".to_owned(),
            alignment: "offset",
            old_bytes: patched.pair.old_bytes,
            new_bytes: patched.pair.new_bytes,
            regions: offset.regions,
            changed_bytes: offset.changed_bytes,
            inserted_bytes: 0,
            refined_bytes: 0,
            first_difference: offset.first_difference,
            timing: offset_timing,
            tracked_peak_bytes: offset.tracked_bytes,
        },
        ByteRow {
            name: "4 KiB insertion at 30 %".to_owned(),
            alignment: "content",
            old_bytes: inserted.pair.old_bytes,
            new_bytes: inserted.pair.new_bytes,
            regions: content.regions,
            changed_bytes: content.inserted_bytes,
            inserted_bytes: content.inserted_bytes,
            refined_bytes: content.refined,
            first_difference: content.first_difference,
            timing: content_timing,
            tracked_peak_bytes: content.tracked_bytes,
        },
    ])
}

fn folder_scenarios(corpus: &generate::Corpus, content_size: u64) -> Result<Vec<SetRow>, String> {
    let large_dir = corpus.dir("content-large")?;
    let small_dir = corpus.dir("content-small")?;
    let large = generate::content_pairs(corpus, "content-large", LARGE_PAIRS, content_size)?;
    let small = generate::content_pairs(corpus, "content-small", SMALL_PAIRS, SMALL_BYTES)?;
    let measured = content::measure(&large, &small, RUNS)?;
    corpus.remove(&large_dir)?;
    corpus.remove(&small_dir)?;
    Ok(vec![
        SetRow {
            set: "large",
            pairs: measured.large.pairs,
            equal_checks: measured.large.equal,
            first_difference: measured.large.first_difference,
            buffered: Timing::of(measured.large.buffered),
            mapped: Timing::of(measured.large.mapped),
            mapped_peak_bytes: measured.large.mapped_peak_bytes,
        },
        SetRow {
            set: "small",
            pairs: measured.small.pairs,
            equal_checks: measured.small.equal,
            first_difference: measured.small.first_difference,
            buffered: Timing::of(measured.small.buffered),
            mapped: Timing::of(measured.small.mapped),
            mapped_peak_bytes: measured.small.mapped_peak_bytes,
        },
    ])
}

pub(super) fn quality_rows() -> Result<Vec<QualityRow>, String> {
    let mut rows = Vec::new();
    for (name, new) in patterns::quality_pairs() {
        let old = base_quality(&name);
        let (anchored, histogram) = quality::quality(&old, &new)?;
        let delta = anchored.abs_diff(histogram);
        let ratio = delta as f64 / histogram.max(1) as f64;
        rows.push(QualityRow {
            name,
            anchored,
            histogram,
            delta,
            ratio,
        });
    }
    Ok(rows)
}

fn base_quality(name: &str) -> String {
    let lines = name
        .split('-')
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut text = String::with_capacity(lines * 64);
    for index in 0..lines {
        text.push_str(&format!(
            "    let field_{index:07} = compute_{:03}(input_{:03}, {});\n",
            index % 211,
            index % 97,
            index % 13
        ));
    }
    text
}

fn sample<T, F>(mut run: F) -> Result<(Timing, T), String>
where
    F: FnMut() -> Result<T, String>,
{
    let mut samples = Vec::with_capacity(RUNS);
    let mut last = None;
    for _ in 0..RUNS {
        let started = Instant::now();
        let value = run()?;
        samples.push(elapsed_ms(started));
        last = Some(value);
    }
    let value = last.ok_or("no samples")?;
    Ok((Timing::of(samples), value))
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}
