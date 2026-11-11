//! Unit tests of the folder map's hunks: one tick per difference row, at that row's own index.

use super::{difference_rows, folder_hunks, hunk_at};
use crate::browser::{RowView, Tone};
use crate::compare_diff::map::HunkKind;

/// A pane row drawn in `tone`; the hunks read the tone and nothing else.
fn row(tone: Tone) -> RowView {
    RowView {
        path: "a.txt".to_owned(),
        name: "a.txt".to_owned(),
        side: None,
        tone,
        status: String::new(),
        gutter: None,
    }
}

#[test]
fn a_difference_tone_makes_a_tick_of_its_kind() {
    let rows = [
        row(Tone::Add),
        row(Tone::Delete),
        row(Tone::Change),
        row(Tone::Conflict),
    ];
    let kinds: Vec<HunkKind> = folder_hunks(&rows).iter().map(|hunk| hunk.kind).collect();
    assert_eq!(
        kinds,
        vec![
            HunkKind::Added,
            HunkKind::Deleted,
            HunkKind::Changed,
            HunkKind::Changed,
        ],
        "Add ticks green, Delete red, Change and Conflict amber"
    );
}

#[test]
fn a_muted_or_busy_row_makes_no_tick() {
    let rows = [row(Tone::Muted), row(Tone::Busy)];
    assert!(folder_hunks(&rows).is_empty());
}

#[test]
fn a_tick_covers_its_own_row_alone() {
    let rows = [
        row(Tone::Muted),
        row(Tone::Change),
        row(Tone::Add),
        row(Tone::Busy),
        row(Tone::Delete),
    ];
    let ranges: Vec<_> = folder_hunks(&rows)
        .iter()
        .map(|hunk| hunk.rows.clone())
        .collect();
    assert_eq!(
        ranges,
        vec![1..2, 2..3, 4..5],
        "one row per tick, at the index the list shows it"
    );
}

/// What a tick index used to cost: count the difference rows before the selected one.
fn scanned(rows: &[RowView], selected: usize) -> Option<usize> {
    let difference = |row: &RowView| {
        matches!(
            row.tone,
            Tone::Add | Tone::Delete | Tone::Change | Tone::Conflict
        )
    };
    let row = rows.get(selected)?;
    difference(row).then(|| {
        rows[..selected]
            .iter()
            .filter(|row| difference(row))
            .count()
    })
}

#[test]
fn the_row_table_finds_the_same_tick_as_the_scan() {
    let rows = [
        row(Tone::Muted),
        row(Tone::Change),
        row(Tone::Busy),
        row(Tone::Add),
        row(Tone::Muted),
        row(Tone::Conflict),
        row(Tone::Delete),
        row(Tone::Busy),
    ];
    let table = difference_rows(&rows);
    assert_eq!(table, vec![1, 3, 5, 6], "the difference rows, in order");
    for at in 0..rows.len() + 1 {
        assert_eq!(
            hunk_at(&table, at),
            scanned(&rows, at),
            "row {at} gets the tick the scan found"
        );
    }
}

#[test]
fn the_table_still_names_the_selected_tick() {
    let rows = [row(Tone::Muted), row(Tone::Change), row(Tone::Conflict)];
    let table = difference_rows(&rows);
    assert_eq!(hunk_at(&table, 0), None, "a quiet row has no tick");
    assert_eq!(hunk_at(&table, 1), Some(0), "the first difference");
    assert_eq!(hunk_at(&table, 2), Some(1), "the second difference");
    assert_eq!(hunk_at(&table, 3), None, "no such row");
}
