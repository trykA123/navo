//! Tests for pair conversion: rows, hunks, facts, spans and card text.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use navo_compare::model::CancelToken;
use navo_compare::text::{
    Alignment, CardLimits, Fallback, Hunk, HunkDigests, Line, LineEnding, Options, Row, Whole, card,
};
use navo_core::paging::TextLayout;
use navo_core::{Location, ProviderPath};
use navo_ui::compare_diff::{CardRow, DiffRow, PairKind, ViewedKey};

use crate::compare::diff::convert::{spans, to_card, to_pair_text};
use crate::compare::diff::load::{LoadInput, Loaded, Side, load_pair};
use crate::compare::local::LocalSide;

fn location(path: &Path) -> Location {
    let text = path.to_str().unwrap_or_default();
    Location::local(ProviderPath::from_text(text)).unwrap()
}

fn write_bytes(root: &Path, rel: &str, bytes: &[u8]) {
    fs::write(root.join(rel), bytes).unwrap();
}

fn local_side(root: &Path, rel: &str) -> Side {
    let size = fs::metadata(root.join(rel)).unwrap().len();
    let side = std::sync::Arc::new(LocalSide::new(location(root)).unwrap());
    let entry = navo_compare::model::SideEntry::new(
        navo_compare::model::RelPath::new(navo_core::ProviderPath::from_text(rel)),
        navo_compare::model::EntryKind::File,
        size,
        None,
    );
    (side, entry)
}

fn input(root: &Path, left: &str, right: &str) -> LoadInput {
    LoadInput {
        left: Some(local_side(root, left)),
        right: Some(local_side(root, right)),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

fn write_lines(root: &Path, rel: &str, lines: &[String]) {
    let mut text = lines.join("\n");
    text.push('\n');
    write_bytes(root, rel, text.as_bytes());
}

#[test]
fn rows_and_hunks_map_one_to_one() {
    let dir = tempfile::tempdir().unwrap();
    let left = ["one", "two", "three", "four", "five", "six", "seven"];
    let right = ["one", "TWO", "three", "four", "five", "SIX", "seven"];
    let left: Vec<String> = left.iter().map(|line| (*line).to_owned()).collect();
    let right: Vec<String> = right.iter().map(|line| (*line).to_owned()).collect();
    write_lines(dir.path(), "left.txt", &left);
    write_lines(dir.path(), "right.txt", &right);
    let loaded = load_pair(
        &input(dir.path(), "left.txt", "right.txt"),
        &CancelToken::new(),
    );
    let alignment = loaded.alignment.as_ref().unwrap();
    let text = to_pair_text(&loaded).unwrap();
    assert_eq!(text.rows.len(), alignment.rows.len());
    assert_eq!(text.hunks.len(), alignment.hunks.len());
    assert_eq!(text.hunks.len(), 2);
    for hunk in &text.hunks {
        assert!(!hunk.rows.is_empty());
        assert!(hunk.digest.is_some());
        assert_eq!((hunk.added, hunk.removed), (1, 1));
        for index in hunk.rows.clone() {
            assert!(!matches!(text.rows[index as usize], DiffRow::Equal(_, _)));
        }
    }
    assert_eq!(text.facts.left_encoding, "compare-encoding-utf-8");
    assert_eq!(text.facts.left_endings, "compare-endings-lf");
    assert_eq!(text.viewed, ViewedKey(loaded.viewed));
}

#[test]
fn the_pair_text_carries_the_byte_start_of_every_left_line() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(dir.path(), "left.txt", b"one\r\ntwo\nthree\n");
    write_bytes(dir.path(), "right.txt", b"one\ntwo\nthree\n");
    let loaded = load_pair(
        &input(dir.path(), "left.txt", "right.txt"),
        &CancelToken::new(),
    );
    let text = to_pair_text(&loaded).unwrap();
    assert_eq!(text.left_starts, vec![0, 5, 9, 15]);
}

#[test]
fn a_token_cap_alignment_has_no_digests() {
    let left = whole("a\n", vec![line("a")]);
    let right = whole("a\n", vec![line("a")]);
    let alignment = Alignment {
        rows: vec![Row::Deleted { left: 0 }, Row::Inserted { right: 0 }],
        hunks: vec![Hunk {
            left_lines: 0..1,
            right_lines: 0..1,
            left_bytes: 0..2,
            right_bytes: 0..2,
        }],
        digests: HunkDigests::default(),
        raw_equal: false,
        display_equal: false,
        fallback: Some(Fallback::TokenCap),
    };
    let loaded = Loaded {
        kind: PairKind::Text,
        left: Some(left),
        right: Some(right),
        alignment: Some(alignment),
        viewed: [7; 64],
        viewable: true,
    };
    let text = to_pair_text(&loaded).unwrap();
    assert_eq!(text.hunks.len(), 1);
    assert!(text.hunks[0].digest.is_none());
    assert_eq!(text.facts.fallbacks, vec!["compare-fallback-token-cap"]);
}

#[test]
fn spans_are_utf8_byte_offsets_on_a_non_ascii_line() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(dir.path(), "left.txt", "éx\n".as_bytes());
    write_bytes(dir.path(), "right.txt", "éy\n".as_bytes());
    let loaded = load_pair(
        &input(dir.path(), "left.txt", "right.txt"),
        &CancelToken::new(),
    );
    let text = to_pair_text(&loaded).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let found = loop {
        let found = spans(&loaded, 0..1);
        if !found.is_empty() {
            break found;
        }
        assert!(
            Instant::now() < deadline,
            "the intraline deadline kept expiring"
        );
        thread::yield_now();
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].row, 0);
    assert!(found[0].left.contains(&(2..3)));
    assert!(found[0].right.contains(&(2..3)));
    for range in found[0].left.iter().chain(&found[0].right) {
        assert!(text.left[0].is_char_boundary(range.start as usize));
        assert!(text.left[0].is_char_boundary(range.end as usize));
    }
    assert!(spans(&loaded, 0..0).is_empty());
}

#[test]
fn card_rows_hold_text_for_shown_rows_only() {
    let dir = tempfile::tempdir().unwrap();
    let left: Vec<String> = (0..31).map(|at| format!("line{at}")).collect();
    let mut right = left.clone();
    right[1] = "changed one".to_owned();
    right[15] = "changed two".to_owned();
    right[29] = "changed three".to_owned();
    write_lines(dir.path(), "left.txt", &left);
    write_lines(dir.path(), "right.txt", &right);
    let loaded = load_pair(
        &input(dir.path(), "left.txt", "right.txt"),
        &CancelToken::new(),
    );
    let alignment = loaded.alignment.as_ref().unwrap();
    let plan = card(
        alignment,
        CardLimits {
            context: 1,
            max_rows: 5,
        },
    );
    assert_eq!(plan.more_hunks, 2);
    let card = to_card(&loaded, &plan);
    assert_eq!(card.kind, PairKind::Text);
    assert_eq!(card.hunks.len(), plan.shown_hunks);
    assert_eq!(card.rows.last(), Some(&CardRow::More { hunks: 2 }));
    assert_eq!((card.added, card.removed), (3, 3));
    let lines: Vec<_> = card
        .rows
        .iter()
        .filter_map(|row| match row {
            CardRow::Line { left, right, .. } => Some((left, right)),
            _ => None,
        })
        .collect();
    assert_eq!(
        lines.len(),
        plan.rows
            .iter()
            .filter(|row| matches!(row, navo_compare::text::CardRow::Row { .. }))
            .count()
    );
    assert!(
        lines
            .iter()
            .all(|(left, right)| left.is_some() || right.is_some())
    );
    assert!(!lines.iter().any(|(left, right)| {
        left.as_deref() == Some("line29") || right.as_deref() == Some("changed three")
    }));
}

#[test]
fn a_binary_pair_has_no_pair_text() {
    let dir = tempfile::tempdir().unwrap();
    write_bytes(dir.path(), "left.bin", b"a\0b");
    write_bytes(dir.path(), "right.bin", b"a\0b");
    let loaded = load_pair(
        &input(dir.path(), "left.bin", "right.bin"),
        &CancelToken::new(),
    );
    assert!(to_pair_text(&loaded).is_none());
    let card = to_card(&loaded, &navo_compare::text::CardPlan::default());
    assert_eq!(card.kind, PairKind::Binary);
    assert!(card.rows.is_empty() && card.hunks.is_empty());
    assert_eq!(card.viewed_key, ViewedKey(loaded.viewed));
}

fn line(text: &str) -> Line {
    Line {
        text: text.to_owned(),
        ending: LineEnding::Lf,
        bom: false,
    }
}

fn whole(bytes: &str, lines: Vec<Line>) -> Whole {
    let mut starts = vec![0u64];
    let mut at = 0u64;
    for line in &lines {
        at += u64::try_from(line.text.len() + line.ending.as_str().len()).unwrap_or(u64::MAX);
        starts.push(at);
    }
    Whole {
        layout: TextLayout::default(),
        lines,
        starts,
        bytes: bytes.as_bytes().to_vec(),
        lossless: true,
    }
}
