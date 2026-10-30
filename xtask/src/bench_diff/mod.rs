mod corpus;
mod history;
mod measure;
mod options;
mod pairing;
mod report;

use std::process::ExitCode;

use measure::LineAlgorithm;
use report::Scenario;

const RUNS: usize = 5;
const HISTORY_PAIRS: usize = 50;
const QUICK_HISTORY_PAIRS: usize = 5;

pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match options::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("bench-diff: {error}");
            eprintln!("usage: cargo xtask bench-diff [--quick] [--json <path>]");
            return ExitCode::from(2);
        }
    };
    match benchmark(&options) {
        Ok(report) => {
            print!("{}", report::render(&report));
            if let Some(path) = &options.json
                && let Err(error) = report::write_json(&report, path)
            {
                eprintln!("bench-diff: {error}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("bench-diff: {error}");
            ExitCode::FAILURE
        }
    }
}

fn benchmark(options: &options::Options) -> Result<report::Report, String> {
    let (history, generated, map, map_lines) = load_corpus(options)?;
    let (subhunks, history_subhunks) = sample_subhunks(&history, &generated);
    if subhunks.is_empty() {
        return Err("could not build a pairing sample".to_owned());
    }
    let rows = intraline_rows(&history, &subhunks)?;
    let scenarios = build_scenarios(&generated, &history, &map, map_lines);
    let threshold_rows = thresholds(&subhunks);
    let best_threshold = best_threshold(&threshold_rows);
    let (word_samples, char_samples, combined_samples) = measure::intraline_samples(&rows, RUNS);
    let intraline_words = report::timing(word_samples);
    let intraline_chars = report::timing(char_samples);
    let intraline_combined = report::timing(combined_samples);
    let pairing = report::timing(measure::pairing_samples(&pairing_fixture(), RUNS));
    let stop = stop_report(&scenarios, &intraline_combined);
    let char_rows = rows
        .iter()
        .filter(|(old, new)| {
            old.chars().count() <= measure::SHORT_LINE && new.chars().count() <= measure::SHORT_LINE
        })
        .count();
    let notes = notes(
        options,
        &history,
        SampleStats {
            history_subhunks,
            sample_subhunks: subhunks.len(),
            intraline_rows: rows.len(),
            char_rows,
        },
        map.old.len(),
    );
    Ok(report::Report {
        quick: options.quick,
        runs: RUNS,
        scenarios,
        intraline_words,
        intraline_chars,
        intraline_combined,
        pairing,
        thresholds: threshold_rows,
        best_threshold,
        stop,
        notes,
    })
}

fn load_corpus(
    options: &options::Options,
) -> Result<
    (
        history::History,
        Vec<corpus::Generated>,
        corpus::Pair,
        usize,
    ),
    String,
> {
    let history_pairs = if options.quick {
        QUICK_HISTORY_PAIRS
    } else {
        HISTORY_PAIRS
    };
    let history = history::load(history_pairs)?;
    let generated = corpus::generated();
    let (map, map_lines) = corpus::map_pair();
    Ok((history, generated, map, map_lines))
}

fn sample_subhunks(
    history: &history::History,
    generated: &[corpus::Generated],
) -> (Vec<pairing::Subhunk>, usize) {
    let mut subhunks = corpus::collect_subhunks(&history.pairs, 12, 50);
    let history_subhunks = subhunks.len();
    for set in generated {
        let needed = 50_usize.saturating_sub(subhunks.len());
        if needed == 0 {
            break;
        }
        subhunks.extend(corpus::collect_subhunks(
            std::slice::from_ref(&set.pair),
            12,
            needed,
        ));
    }
    (subhunks, history_subhunks)
}

fn build_scenarios(
    generated: &[corpus::Generated],
    history: &history::History,
    map: &corpus::Pair,
    map_lines: usize,
) -> Vec<Scenario> {
    let mut scenarios: Vec<Scenario> = generated
        .iter()
        .map(|set| scenario(set.name, 1, set.lines, std::slice::from_ref(&set.pair)))
        .collect();
    scenarios.push(scenario(
        "history",
        history.pairs.len(),
        history
            .pairs
            .iter()
            .map(|pair| pair.old.lines().count())
            .sum(),
        &history.pairs,
    ));
    scenarios.push(scenario(
        "map-50mb",
        1,
        map_lines,
        std::slice::from_ref(map),
    ));
    scenarios
}

fn best_threshold(rows: &[report::ThresholdRow]) -> f64 {
    rows.iter()
        .max_by(|a, b| {
            a.f1.total_cmp(&b.f1)
                .then_with(|| b.threshold.total_cmp(&a.threshold))
        })
        .map_or(0.0, |row| row.threshold)
}

fn scenario(name: &str, files: usize, lines: usize, pairs: &[corpus::Pair]) -> Scenario {
    let algorithms = LineAlgorithm::ALL
        .into_iter()
        .map(|algorithm| {
            report::algorithm(
                algorithm.name(),
                measure::line_samples(algorithm, pairs, RUNS),
            )
        })
        .collect();
    Scenario {
        name: name.to_owned(),
        files,
        lines,
        algorithms,
    }
}

fn thresholds(subhunks: &[pairing::Subhunk]) -> Vec<report::ThresholdRow> {
    let samples = pairing::labelled(subhunks);
    (8..=16)
        .map(|step| pairing::metrics(&samples, f64::from(step) * 0.05))
        .map(|metrics| report::ThresholdRow {
            threshold: metrics.threshold,
            precision: metrics.precision,
            recall: metrics.recall,
            f1: metrics.f1,
            true_positives: metrics.true_positives,
            false_positives: metrics.false_positives,
            false_negatives: metrics.false_negatives,
        })
        .collect()
}

fn intraline_rows(
    history: &history::History,
    sample: &[pairing::Subhunk],
) -> Result<Vec<(String, String)>, String> {
    let mut rows = Vec::new();
    collect_rows(sample, &mut rows);
    if rows.len() < 60 {
        let more = corpus::collect_subhunks(&history.pairs, 40, 200);
        collect_rows(&more, &mut rows);
    }
    rows.truncate(60);
    if rows.len() < 60 {
        return Err(format!(
            "needed 60 paired intraline rows, found {}",
            rows.len()
        ));
    }
    Ok(rows)
}

fn collect_rows(subhunks: &[pairing::Subhunk], rows: &mut Vec<(String, String)>) {
    for subhunk in subhunks {
        for pair in pairing::pair_rows(subhunk, 0.6) {
            if rows.len() >= 60 {
                return;
            }
            if pair.0.chars().count() <= 4096 && pair.1.chars().count() <= 4096 {
                rows.push(pair);
            }
        }
    }
}

fn pairing_fixture() -> pairing::Subhunk {
    let old: Vec<String> = (0..200)
        .map(|index| {
            format!(
                "    let value_{index:03} = compute_{:02}(input_{:02});",
                index % 37,
                index % 11
            )
        })
        .collect();
    let new = old
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index % 4 == 0 {
                format!("{line} // revised")
            } else {
                line.clone()
            }
        })
        .collect();
    pairing::Subhunk { old, new }
}

fn stop_report(scenarios: &[Scenario], intraline: &report::Timing) -> report::StopReport {
    let ten_k: Vec<&Scenario> = scenarios
        .iter()
        .filter(|scenario| scenario.name.starts_with("generated-10k-"))
        .collect();
    let mut slower = false;
    let mut worst_histogram: f64 = 0.0;
    for scenario in ten_k {
        let Some(histogram) = scenario
            .algorithms
            .iter()
            .find(|algorithm| algorithm.algorithm == "imara-histogram")
        else {
            continue;
        };
        let similar = scenario
            .algorithms
            .iter()
            .filter(|algorithm| algorithm.algorithm.starts_with("similar-"))
            .map(|algorithm| algorithm.median_ms)
            .fold(f64::INFINITY, f64::min);
        slower |= histogram.median_ms > similar;
        worst_histogram = worst_histogram.max(histogram.median_ms);
    }
    let combined = worst_histogram + intraline.median_ms;
    report::StopReport {
        imara_histogram_slower_than_similar_10k: slower,
        line_plus_intraline_ms: combined,
        line_plus_intraline_over_50_ms: combined > 50.0,
    }
}

#[derive(Clone, Copy)]
struct SampleStats {
    history_subhunks: usize,
    sample_subhunks: usize,
    intraline_rows: usize,
    char_rows: usize,
}

fn notes(
    options: &options::Options,
    history: &history::History,
    stats: SampleStats,
    map_bytes: usize,
) -> Vec<String> {
    let mut notes = vec![
        format!(
            "History corpus: {} text file pair(s) from {} first-parent commit pair(s); {} binary, non-UTF-8, non-blob or unchanged record(s) skipped.",
            history.pairs.len(),
            history.commits,
            history.skipped
        ),
        format!(
            "Threshold sample: {}/{} subhunks came from history (the rest from generated files), each capped at 12 lines per side. A pair is labelled positive when the lines' lower-case alphanumeric word sets have Jaccard >= 0.5. This sample still needs hand-checking.",
            stats.history_subhunks, stats.sample_subhunks
        ),
        format!(
            "Intraline: {} paired rows at 60 %; chars on {} short row(s) (both lines <= 80 characters); similar's word/char diffs have a 5 ms deadline.",
            stats.intraline_rows, stats.char_rows
        ),
        "Pairing distance is Levenshtein over chars; a greedy pass takes pairs in ascending raw distance and rejects a pair above the threshold fraction of the longer line.".to_owned(),
        format!(
            "MAP-like file: {map_bytes} bytes, 0.01 % of lines changed, generated deterministically."
        ),
    ];
    if options.quick {
        notes.push(format!(
            "Quick mode: {QUICK_HISTORY_PAIRS} history pairs instead of {HISTORY_PAIRS}; all other scenarios and all {RUNS} runs are unchanged."
        ));
    }
    notes
}
