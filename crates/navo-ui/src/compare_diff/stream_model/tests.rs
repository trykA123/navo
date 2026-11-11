//! Tests for the Stream model: item filter, J/K and Shift+J/K stops, folds and Viewed. Shared helpers.

use super::*;
use crate::browser::{CompareKind, CompareRow, CompareSide, RowStatus};
use crate::compare_diff::hooks::{HunkView, PairKind, StreamUpdate};

pub(super) fn side(kind: CompareKind) -> Option<CompareSide> {
    Some(CompareSide {
        kind,
        size: 1,
        modified_ms: None,
    })
}

pub(super) fn row(id: u64, path: &str, status: RowStatus, kind: CompareKind) -> CompareRow {
    CompareRow::new(id, path, status, (side(kind), side(kind)))
}

pub(super) fn file(id: u64, path: &str, status: RowStatus) -> CompareRow {
    row(id, path, status, CompareKind::File)
}

pub(super) fn hunk() -> HunkView {
    HunkView {
        rows: 0..1,
        left_bytes: 0..1,
        right_bytes: 0..1,
        digest: None,
        added: 1,
        removed: 0,
    }
}

pub(super) fn card(hunks: usize, key: u8) -> Arc<Card> {
    Arc::new(Card {
        kind: PairKind::Text,
        rows: vec![],
        hunks: vec![hunk(); hunks],
        added: 0,
        removed: 0,
        viewed_key: ViewedKey([key; 64]),
        viewed: false,
        viewable: true,
    })
}

pub(super) fn model(ids: &[u64]) -> StreamModel {
    let rows: Vec<CompareRow> = ids
        .iter()
        .map(|&id| file(id, &format!("f{id}.txt"), RowStatus::Different))
        .collect();
    let mut m = StreamModel::default();
    m.set_items(1, stream_items(&rows, ""));
    m
}

pub(super) fn give(m: &mut StreamModel, id: u64, hunks: usize) {
    m.apply(&StreamUpdate::Card {
        generation: m.generation,
        id,
        card: card(hunks, id as u8),
    });
}

#[test]
fn items_keep_files_under_the_folder_in_order() {
    let rows = vec![
        row(1, "d", RowStatus::Different, CompareKind::Folder),
        file(2, "d/a.txt", RowStatus::Different),
        file(3, "d/b.txt", RowStatus::Same),
        file(4, "d/c.txt", RowStatus::SameAfterRules),
        file(5, "d/e.txt", RowStatus::Checking),
        file(6, "x/z.txt", RowStatus::OnlyLeft),
        file(7, "d/f.txt", RowStatus::Unavailable("why")),
        file(8, "d/g.txt", RowStatus::SameSizeTime),
        file(9, "dx/a.txt", RowStatus::Different),
        file(10, "d/sub/deep.txt", RowStatus::CaseConflict),
        row(11, "d/link", RowStatus::Different, CompareKind::Link),
        CompareRow {
            left: side(CompareKind::Folder),
            ..file(12, "d/kind", RowStatus::TypeConflict)
        },
    ];
    let list = stream_items(&rows, "d");
    let ids: Vec<u64> = list.items.iter().map(|i| i.id).collect();
    // The compare's own order is kept; folders, links, Same rows and other folders are out.
    assert_eq!(ids, [2, 5, 7, 10, 12]);
    assert!(!list.items[1].settled);
    assert!(list.items[0].settled);
    assert_eq!(list.rules, 1);
    assert_eq!(stream_items(&rows, "").items.len(), 7);
}

#[test]
fn j_and_k_walk_hunks_and_hunkless_cards() {
    let mut m = model(&[1, 2, 3]);
    give(&mut m, 1, 2);
    give(&mut m, 3, 1);
    // Item 2 has no card yet: one stop.
    assert_eq!(m.stops().len(), 4);
    m.cursor = m.stops()[0];
    let mut seen = vec![m.cursor];
    while m.next_change(1) {
        seen.push(m.cursor);
    }
    let hunks: Vec<(usize, Option<u32>)> = seen.iter().map(|s| (s.item, s.hunk)).collect();
    assert_eq!(hunks, [(0, Some(0)), (0, Some(1)), (1, None), (2, Some(0))]);
    assert!(m.next_change(-1));
    assert_eq!(m.cursor.item, 1);
}

#[test]
fn shift_j_moves_by_file() {
    let mut m = model(&[1, 2, 3]);
    give(&mut m, 1, 2);
    give(&mut m, 2, 2);
    give(&mut m, 3, 2);
    m.cursor = Stop {
        item: 0,
        hunk: Some(1),
    };
    assert!(m.next_file(1));
    assert_eq!(
        m.cursor,
        Stop {
            item: 1,
            hunk: Some(0)
        }
    );
    assert!(m.next_file(-1));
    assert_eq!(
        m.cursor,
        Stop {
            item: 0,
            hunk: Some(0)
        }
    );
    assert!(!m.next_file(-1));
}

#[test]
fn viewed_moves_on_and_counts() {
    let mut m = model(&[1, 2]);
    give(&mut m, 1, 2);
    give(&mut m, 2, 1);
    m.cursor = Stop {
        item: 0,
        hunk: Some(0),
    };
    let (id, key, now) = m.toggle_viewed().expect("card present");
    assert_eq!((id, key, now), (1, ViewedKey([1; 64]), true));
    assert_eq!(m.cursor.item, 1);
    assert_eq!(m.counter(), (1, 2));
    // A viewed card is one stop.
    assert_eq!(m.stops().len(), 2);
    m.cursor = Stop {
        item: 0,
        hunk: None,
    };
    let (_, _, now) = m.toggle_viewed().expect("card present");
    assert!(!now);
    assert_eq!(m.counter(), (0, 2));
    assert_eq!(
        m.cursor,
        Stop {
            item: 0,
            hunk: Some(0)
        }
    );
}

#[test]
fn viewed_without_a_card_does_nothing() {
    let mut m = model(&[1]);
    assert_eq!(m.toggle_viewed(), None);
}

#[test]
fn viewed_on_an_unhashed_card_does_nothing() {
    let mut m = model(&[1, 2]);
    let mut unhashed = (*card(1, 0)).clone();
    unhashed.viewable = false;
    m.apply(&StreamUpdate::Card {
        generation: m.generation,
        id: 1,
        card: Arc::new(unhashed),
    });
    assert_eq!(m.toggle_viewed(), None);
    assert!(!m.is_viewed(1));
    assert_eq!(m.cursor.item, 0);
}

#[test]
fn fold_makes_one_stop() {
    let mut m = model(&[1]);
    give(&mut m, 1, 3);
    assert_eq!(m.stops().len(), 3);
    m.toggle_fold();
    assert_eq!(
        m.stops(),
        [Stop {
            item: 0,
            hunk: None
        }]
    );
    m.toggle_fold();
    assert_eq!(m.stops().len(), 3);
    // Opening again puts the cursor back on a hunk, so J and K keep working.
    assert_eq!(
        m.cursor,
        Stop {
            item: 0,
            hunk: Some(0)
        }
    );
}

#[test]
fn o_opens_a_viewed_card_without_unmarking_it() {
    let mut m = model(&[1]);
    give(&mut m, 1, 2);
    m.toggle_viewed();
    assert!(!m.is_open(1));
    m.toggle_fold();
    assert!(m.is_open(1));
    assert!(m.is_viewed(1));
    assert_eq!(m.stops().len(), 2);
    // Unmarking resets the fold to the default: open.
    m.toggle_viewed();
    assert!(m.is_open(1));
    m.toggle_viewed();
    assert!(!m.is_open(1));
}

#[test]
fn k_crosses_back_into_the_previous_file_and_ends_stop() {
    let mut m = model(&[1, 2]);
    give(&mut m, 1, 2);
    give(&mut m, 2, 1);
    m.cursor = Stop {
        item: 1,
        hunk: Some(0),
    };
    assert!(!m.next_change(1));
    assert!(m.next_change(-1));
    assert_eq!(
        m.cursor,
        Stop {
            item: 0,
            hunk: Some(1)
        }
    );
    m.cursor = m.stops()[0];
    assert!(!m.next_change(-1));
}

#[test]
fn viewed_on_the_last_file_stays_there() {
    let mut m = model(&[1, 2]);
    give(&mut m, 2, 2);
    m.cursor = Stop {
        item: 1,
        hunk: Some(1),
    };
    m.toggle_viewed();
    assert_eq!(
        m.cursor,
        Stop {
            item: 1,
            hunk: None
        }
    );
    assert_eq!(m.counter(), (1, 2));
}
