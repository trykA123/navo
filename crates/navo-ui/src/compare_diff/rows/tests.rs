//! Tests for the row builders: fillers, unified order, ranges, Stream rows, and the 2 ms budget.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::browser::{CompareKind, CompareRow, CompareSide, RowStatus};
use crate::compare_diff::hooks::{Card, HunkView, PairFacts, PairKind, StreamUpdate, ViewedKey};
use crate::compare_diff::stream_model::stream_items;

/// A pair whose side lines are "l0", "l1"… and "r0", "r1"…, with the given rows and hunks.
fn text(rows: Vec<DiffRow>, hunks: &[(u32, u32)], lines: usize) -> PairText {
    let hunk = |&(start, end): &(u32, u32)| HunkView {
        rows: start..end,
        left_bytes: 0..0,
        right_bytes: 0..0,
        digest: None,
        added: 0,
        removed: 0,
    };
    PairText {
        left_starts: Vec::new(),
        left: (0..lines).map(|i| format!("l{i}")).collect(),
        right: (0..lines).map(|i| format!("r{i}")).collect(),
        rows,
        hunks: hunks.iter().map(hunk).collect(),
        facts: PairFacts {
            left_encoding: "",
            right_encoding: "",
            left_endings: "",
            right_endings: "",
            raw_equal: false,
            display_equal: false,
            fallbacks: vec![],
        },
        viewed: ViewedKey([0; 64]),
    }
}

/// Equal 0, changed 1–2, deleted 3, inserted 3, equal 4/4; one hunk over rows 1..5.
fn sample() -> PairText {
    use DiffRow::{Changed, Deleted, Equal, Inserted};
    let rows = vec![
        Equal(0, 0),
        Changed(1, 1),
        Changed(2, 2),
        Deleted(3),
        Inserted(3),
        Equal(4, 4),
    ];
    text(rows, &[(1, 5)], 5)
}

/// Each row as a short code: kind letter, left text, right text.
fn codes(rows: &[ShownRow<'_>]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            let kind = format!("{:?}", row.kind);
            let side = |s: Option<(u32, &str)>| s.map_or("-", |(_, t)| t).to_owned();
            format!("{}:{}:{}", &kind[..1], side(row.left), side(row.right))
        })
        .collect()
}

#[test]
fn side_by_side_puts_fillers_opposite_lone_lines() {
    let t = sample();
    let rows = pair_rows(&t, Layout::SideBySide, 0..10);
    assert_eq!(
        codes(&rows),
        [
            "E:l0:r0", "C:l1:r1", "C:l2:r2", "D:l3:-", "I:-:r3", "E:l4:r4"
        ]
    );
    assert_eq!(rows[0].hunk, None);
    assert_eq!(rows[3].hunk, Some(0));
    assert_eq!(rows[3].left, Some((3, "l3")));
    assert_eq!(row_count(&t, Layout::SideBySide), 6);
}

#[test]
fn unified_lists_a_hunks_deleted_lines_before_its_inserted_lines() {
    let t = sample();
    let rows = pair_rows(&t, Layout::Unified, 0..100);
    assert_eq!(
        codes(&rows),
        [
            "E:l0:r0", "D:l1:-", "D:l2:-", "D:l3:-", "I:-:r1", "I:-:r2", "I:-:r3", "E:l4:r4"
        ]
    );
    assert!(rows[1..7].iter().all(|r| r.hunk == Some(0)));
    assert_eq!(row_count(&t, Layout::Unified), 8);
}

#[test]
fn ranges_at_the_end_and_inside_a_run_are_cut_exactly() {
    let t = sample();
    assert_eq!(
        codes(&pair_rows(&t, Layout::Unified, 6..100)),
        ["I:-:r3", "E:l4:r4"]
    );
    assert_eq!(
        codes(&pair_rows(&t, Layout::Unified, 2..5)),
        ["D:l2:-", "D:l3:-", "I:-:r1"]
    );
    assert!(pair_rows(&t, Layout::Unified, 8..20).is_empty());
    assert!(pair_rows(&t, Layout::SideBySide, 9..20).is_empty());
    assert_eq!(pair_rows(&t, Layout::SideBySide, 5..20).len(), 1);
}

#[test]
fn every_unified_window_matches_the_whole_list() {
    use DiffRow::{Changed, Deleted, Equal, Inserted};
    // Two hunks, one with an equal row inside it, and plain rows around them.
    let rows = vec![
        Equal(0, 0),
        Changed(1, 1),
        Inserted(2),
        Equal(2, 3),
        Deleted(3),
        Equal(4, 4),
        Equal(5, 5),
        Changed(6, 6),
        Equal(7, 7),
    ];
    let t = text(rows, &[(1, 5), (7, 8)], 8);
    let all = pair_rows(&t, Layout::Unified, 0..usize::MAX);
    assert_eq!(all.len(), row_count(&t, Layout::Unified));
    for start in 0..=all.len() {
        for end in start..=all.len() + 1 {
            let part = pair_rows(&t, Layout::Unified, start..end);
            assert_eq!(
                part,
                all[start..end.min(all.len())],
                "window {start}..{end}"
            );
        }
    }
}

/// A Stream with three different files, rules group of 2, and the given cards.
fn stream(cards: &[(u64, Card)]) -> StreamModel {
    let side = Some(CompareSide {
        kind: CompareKind::File,
        size: 1,
        modified_ms: None,
    });
    let row = |id: u64, status| CompareRow {
        id,
        path: format!("f{id}"),
        left: side,
        right: side,
        status,
        contains_differences: false,
    };
    let rows = [
        row(1, RowStatus::Different),
        row(2, RowStatus::Different),
        row(3, RowStatus::Different),
        row(4, RowStatus::SameAfterRules),
        row(5, RowStatus::SameAfterRules),
    ];
    let mut m = StreamModel::default();
    m.set_items(1, stream_items(&rows, ""));
    for (id, card) in cards {
        m.apply(&StreamUpdate::Card {
            generation: 1,
            id: *id,
            card: Arc::new(card.clone()),
        });
    }
    m
}

/// A card of `kind` with one changed line, a gap and a "more" row, or no rows.
fn card(kind: PairKind, with_rows: bool, viewed: bool) -> Card {
    let rows = if with_rows {
        vec![
            CardRow::Line {
                row: DiffRow::Changed(0, 0),
                hunk: 0,
                left: Some("a".into()),
                right: Some("b".into()),
            },
            CardRow::Gap { lines: 7 },
            CardRow::More { hunks: 2 },
        ]
    } else {
        vec![]
    };
    Card {
        kind,
        rows,
        hunks: vec![],
        added: 1,
        removed: 1,
        viewed_key: ViewedKey([1; 64]),
        viewed,
        viewable: true,
    }
}

#[test]
fn stream_rows_cover_open_viewed_placeholder_summary_and_rules() {
    let mut m = stream(&[
        (1, card(PairKind::Text, true, false)),
        (2, card(PairKind::Text, true, true)),
    ]);
    m.apply(&StreamUpdate::Card {
        generation: 1,
        id: 3,
        card: Arc::new(card(PairKind::Binary, false, false)),
    });
    use StreamRow::*;
    assert_eq!(
        stream_rows(&m, Layout::SideBySide),
        [
            Header(0),
            Line {
                item: 0,
                card_row: 0,
                half: Half::Both
            },
            Gap { item: 0, lines: 7 },
            More { item: 0, hunks: 2 },
            Header(1),
            Header(2),
            Summary(2),
            RulesGroup,
        ]
    );
    // Folding the first card leaves its header; an unbuilt card shows a placeholder.
    m.toggle_fold();
    m.apply(&StreamUpdate::Evicted {
        generation: 1,
        id: 3,
    });
    assert_eq!(
        stream_rows(&m, Layout::Unified),
        [Header(0), Header(1), Header(2), Placeholder(2), RulesGroup]
    );
    m.begin_search(1);
    assert!(stream_rows(&m, Layout::Unified).is_empty());
}

/// A card line with the given row, hunk and side texts.
fn line(row: DiffRow, hunk: u32) -> CardRow {
    CardRow::Line {
        row,
        hunk,
        left: Some("l".into()),
        right: Some("r".into()),
    }
}

#[test]
fn unified_cards_list_deleted_lines_before_inserted_ones_per_hunk() {
    let rows = vec![
        line(DiffRow::Equal(0, 0), 0),
        line(DiffRow::Changed(1, 1), 0),
        line(DiffRow::Deleted(2), 0),
        line(DiffRow::Inserted(2), 0),
        line(DiffRow::Equal(3, 3), 0),
        CardRow::Gap { lines: 4 },
        line(DiffRow::Inserted(9), 1),
    ];
    let mut c = card(PairKind::Text, false, false);
    c.rows = rows;
    let m = stream(&[(1, c)]);
    let shown: Vec<_> = stream_rows(&m, Layout::Unified)
        .into_iter()
        .filter_map(|row| match row {
            StreamRow::Line { card_row, half, .. } => Some((card_row, half)),
            _ => None,
        })
        .collect();
    use Half::*;
    assert_eq!(
        shown,
        [
            (0, Both),
            (1, Left),
            (2, Left),
            (1, Right),
            (3, Right),
            (4, Both),
            (6, Right)
        ]
    );
    let side: Vec<_> = stream_rows(&m, Layout::SideBySide)
        .into_iter()
        .filter(|row| matches!(row, StreamRow::Line { half: Both, .. }))
        .collect();
    assert_eq!(side.len(), 6);
}

/// A 1,000,000-row pair: a 4-row changed hunk every 50 rows, or every row changed.
fn big(dense: bool) -> PairText {
    const ROWS: u32 = 1_000_000;
    let changed = |i: u32| dense || i % 50 < 4;
    let rows = (0..ROWS)
        .map(|i| {
            if changed(i) {
                DiffRow::Changed(i, i)
            } else {
                DiffRow::Equal(i, i)
            }
        })
        .collect();
    let hunks: Vec<(u32, u32)> = if dense {
        vec![(0, ROWS)]
    } else {
        (0..ROWS).step_by(50).map(|s| (s, s + 4)).collect()
    };
    text(rows, &hunks, ROWS as usize)
}

/// The fastest of five builds of the last 60 rows in `layout`.
fn last_window(t: &PairText, layout: Layout) -> Duration {
    let total = row_count(t, layout);
    (0..5)
        .map(|_| {
            let started = Instant::now();
            let rows = pair_rows(t, layout, total - 60..total);
            let took = started.elapsed();
            assert_eq!(rows.len(), 60);
            took
        })
        .min()
        .unwrap_or_default()
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "timing: release only; run with --release --run-ignored all"
)]
fn sixty_rows_of_a_million_build_under_two_ms() {
    let budget = Duration::from_millis(2);
    for dense in [false, true] {
        let t = big(dense);
        for layout in [Layout::SideBySide, Layout::Unified] {
            let took = last_window(&t, layout);
            assert!(
                took < budget,
                "{layout:?} dense={dense}: {took:?} breaks the frame budget"
            );
        }
    }
}
