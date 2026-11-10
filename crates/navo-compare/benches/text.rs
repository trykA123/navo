//! Instruction-count benches for compare text: line diff, card plan and intraline.
//! No gungraun limits are set: `cargo xtask speed-verdict` judges the summaries.

#![allow(
    clippy::unwrap_used,
    reason = "a broken fixture must stop the run, not measure an error path"
)]

use std::hint::black_box;

use gungraun::{
    Callgrind, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main,
};
use navo_compare::text::intraline::{SideLine, intraline_unbounded};
use navo_compare::text::side::LineSet;
use navo_compare::text::{
    Alignment, CardLimits, CardPlan, Line, LineEnding, LinePair, Options, Row, card, diff_lines,
};
use navo_core::paging::TextLayout;

const LINES: usize = 20_000;
const CHANGED_EVERY: usize = 100;
const REMOVED_EVERY: usize = 250;
const INSERTED_EVERY: usize = 400;

/// The two sides, their line starts, and the right line for each surviving left line.
struct Fixture {
    left: Vec<Line>,
    left_starts: Vec<u64>,
    right: Vec<Line>,
    right_starts: Vec<u64>,
    right_of_left: Vec<Option<usize>>,
}

/// A left line and its edited right line with their source starts.
struct OwnedPair {
    left: Line,
    right: Line,
    left_start: u64,
    right_start: u64,
}

/// Builds the 20,000-line pair: 200 edits, 80 removals and 50 insertions.
fn fixture() -> Fixture {
    let mut left = Vec::with_capacity(LINES);
    for i in 0..LINES {
        let argument = i * 7 % 13;
        left.push(Line {
            text: format!("    value_{i} = compute({i}, {argument});"),
            ending: LineEnding::Lf,
            bom: false,
        });
    }
    let left_starts = starts(&left);

    let mut right = Vec::with_capacity(LINES + 50 - 80);
    let mut right_of_left = vec![None; LINES];
    for (i, line) in left.iter().enumerate() {
        if i % REMOVED_EVERY == 125 {
            continue;
        }
        let text = if i % CHANGED_EVERY == 50 {
            format!("{} // edited", line.text)
        } else {
            line.text.clone()
        };
        right_of_left[i] = Some(right.len());
        right.push(Line {
            text,
            ending: LineEnding::Lf,
            bom: false,
        });
        if i % INSERTED_EVERY == 0 {
            right.push(Line {
                text: format!("    inserted_{i}();"),
                ending: LineEnding::Lf,
                bom: false,
            });
        }
    }
    let right_starts = starts(&right);
    Fixture {
        left,
        left_starts,
        right,
        right_starts,
        right_of_left,
    }
}

/// Returns the cumulative source starts, including the final end offset.
fn starts(lines: &[Line]) -> Vec<u64> {
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut at = 0u64;
    starts.push(at);
    for line in lines {
        at += (line.text.len() + line.ending.as_str().len()) as u64;
        starts.push(at);
    }
    starts
}

/// Wraps one side for `diff_lines`; the raw bytes are not needed for these benches.
fn set<'a>(lines: &'a [Line], starts: &'a [u64]) -> LineSet<'a> {
    LineSet {
        lines,
        starts,
        bom: false,
        bytes: None,
    }
}

/// Counts the paired changed rows.
fn changed_rows(alignment: &Alignment) -> usize {
    alignment
        .rows
        .iter()
        .filter(|row| matches!(row, Row::Changed { .. }))
        .count()
}

/// Builds the fixture and asserts that the 200 edited lines are paired changed rows.
fn diff_case() -> Fixture {
    let fixture = fixture();
    let alignment = diff_lines(
        set(&fixture.left, &fixture.left_starts),
        set(&fixture.right, &fixture.right_starts),
        &Options::default(),
    );
    assert_eq!(
        changed_rows(&alignment),
        200,
        "the edited lines must be changed rows"
    );
    fixture
}

/// Builds an alignment for the card bench and asserts that it has hunks.
fn card_case() -> Alignment {
    let fixture = fixture();
    let alignment = diff_lines(
        set(&fixture.left, &fixture.left_starts),
        set(&fixture.right, &fixture.right_starts),
        &Options::default(),
    );
    assert_eq!(
        changed_rows(&alignment),
        200,
        "the edited lines must be changed rows"
    );
    assert!(!alignment.hunks.is_empty(), "the card has no hunk");
    alignment
}

/// Builds the 200 changed pairs and asserts that each one refines fully.
fn intraline_case() -> Vec<OwnedPair> {
    let fixture = fixture();
    let mut pairs = Vec::with_capacity(200);
    for i in (0..LINES).filter(|i| i % CHANGED_EVERY == 50) {
        let right = fixture.right_of_left[i].unwrap();
        pairs.push(OwnedPair {
            left: fixture.left[i].clone(),
            right: fixture.right[right].clone(),
            left_start: fixture.left_starts[i],
            right_start: fixture.right_starts[right],
        });
    }
    assert_eq!(pairs.len(), 200, "the fixture must give 200 changed pairs");
    for pair in &pairs {
        let result = intraline_unbounded(pair_of(pair));
        assert_eq!(result.fallback, None, "a changed pair hit a fallback");
        assert!(
            !result.words.is_empty() || !result.chars.is_empty(),
            "a changed pair has no span"
        );
    }
    pairs
}

/// Builds a borrowed line pair from an owned fixture pair.
fn pair_of(owned: &OwnedPair) -> LinePair<'_> {
    LinePair {
        left: SideLine {
            line: &owned.left,
            start: owned.left_start,
            layout: TextLayout::default(),
            lossless: true,
        },
        right: SideLine {
            line: &owned.right,
            start: owned.right_start,
            layout: TextLayout::default(),
            lossless: true,
        },
    }
}

#[library_benchmark]
#[bench::twenty_thousand(diff_case())]
fn bench_diff(fixture: Fixture) -> Alignment {
    black_box(diff_lines(
        set(&fixture.left, &fixture.left_starts),
        set(&fixture.right, &fixture.right_starts),
        &Options::default(),
    ))
}

#[library_benchmark]
#[bench::default_limits(card_case())]
fn bench_card(alignment: Alignment) -> CardPlan {
    black_box(card(&alignment, CardLimits::default()))
}

#[library_benchmark]
#[bench::two_hundred(intraline_case())]
fn bench_intraline(pairs: Vec<OwnedPair>) -> usize {
    let mut spans = 0;
    for pair in &pairs {
        let result = intraline_unbounded(pair_of(pair));
        spans += black_box(result.words.len() + result.chars.len());
    }
    black_box(spans)
}

/// Callgrind with default options and no soft limits.
fn callgrind() -> Callgrind {
    Callgrind::default()
}

/// Bench config: Callgrind plus one rayon thread, so counts do not depend on scheduling.
fn config() -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config.tool(callgrind()).env("RAYON_NUM_THREADS", "1");
    config
}

library_benchmark_group!(
    name = text,
    benchmarks = [bench_diff, bench_card, bench_intraline]
);

main!(config = config(), library_benchmark_groups = text);
