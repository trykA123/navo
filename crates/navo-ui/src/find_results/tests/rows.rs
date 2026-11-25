//! Row building tests for the find results window.

use navo_core::find::{FileHits, LineHit};
use navo_core::{EntryKind, ProviderPath};
use navo_fs::find::{FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use crate::find_results::rows::{LinesShown, Row, RowMode, SortKey, index_of, nearest, rows};
use crate::find_results::store::HitStore;

/// One found file with `lines` stored hit lines and `total` counted hits.
fn found(relative: &str, size: u64, lines: u64, total: u32) -> FoundFile {
    FoundFile {
        relative: ProviderPath::from_text(relative),
        entry: FsEntry {
            name: ProviderPath::from_text(relative),
            kind: EntryKind::File,
            size,
            modified_ms: Some(100 - i64::try_from(size).unwrap_or_default()),
            attributes: Attributes::default(),
        },
        hits: FileHits {
            hits: (1..=lines)
                .map(|line| LineHit {
                    line,
                    column: 0,
                    text: "needle".into(),
                })
                .collect(),
            total,
            skipped: None,
        },
    }
}

fn store(files: Vec<FoundFile>) -> HitStore {
    let mut store = HitStore::default();
    store
        .push(
            0,
            &FoundBatch {
                files,
                progress: FindProgress::default(),
            },
        )
        .expect("a small batch fits");
    store
}

/// Files in arrival order: `b/` is seen before `a/`; every key orders them differently.
fn sample() -> HitStore {
    store(vec![
        found("b/aaa.rs", 15, 5, 5),
        found("a/alpha.rs", 10, 1, 1),
        found("b/beta.rs", 20, 2, 2),
    ])
}

fn mode(grouped: bool, lines: LinesShown, sort: Option<(SortKey, bool)>) -> RowMode {
    RowMode {
        grouped,
        lines,
        sort,
    }
}

fn files(rows: &[Row]) -> Vec<u32> {
    rows.iter()
        .filter_map(|row| match row {
            Row::File(file) => Some(*file),
            _ => None,
        })
        .collect()
}

#[test]
fn flat_rows_keep_discovery_order_without_lines() {
    let built = rows(&sample(), &mode(false, LinesShown::Off, None));
    assert_eq!(built, vec![Row::File(0), Row::File(1), Row::File(2)]);
}

#[test]
fn grouped_rows_put_each_folder_before_its_files_by_first_sight() {
    let built = rows(&sample(), &mode(true, LinesShown::Off, None));
    assert_eq!(
        built,
        vec![
            Row::Folder(0),
            Row::File(0),
            Row::File(2),
            Row::Folder(1),
            Row::File(1),
        ]
    );
}

#[test]
fn three_lines_then_more_when_the_file_has_more() {
    let built = rows(&sample(), &mode(false, LinesShown::On, None));
    assert_eq!(
        built,
        vec![
            Row::File(0),
            Row::Line { file: 0, nth: 0 },
            Row::Line { file: 0, nth: 1 },
            Row::Line { file: 0, nth: 2 },
            Row::More { file: 0, hidden: 2 },
            Row::File(1),
            Row::Line { file: 1, nth: 0 },
            Row::File(2),
            Row::Line { file: 2, nth: 0 },
            Row::Line { file: 2, nth: 1 },
        ]
    );
}

#[test]
fn auto_lines_turn_off_above_twenty_thousand_hits() {
    let few = store(vec![found("a.rs", 1, 1, 20_000)]);
    assert_eq!(
        rows(&few, &mode(false, LinesShown::Auto, None)),
        vec![
            Row::File(0),
            Row::Line { file: 0, nth: 0 },
            Row::More {
                file: 0,
                hidden: 19_999
            },
        ]
    );
    let many = store(vec![found("a.rs", 1, 1, 20_001)]);
    assert_eq!(
        rows(&many, &mode(false, LinesShown::Auto, None)),
        vec![Row::File(0)]
    );
}

#[test]
fn each_sort_key_orders_files_both_ways() {
    let sample = sample();
    for (key, ascending) in [
        (SortKey::Name, vec![0, 1, 2]),
        (SortKey::Hits, vec![1, 2, 0]),
        (SortKey::Size, vec![1, 0, 2]),
        (SortKey::Modified, vec![2, 0, 1]),
    ] {
        let up = rows(&sample, &mode(false, LinesShown::Off, Some((key, false))));
        assert_eq!(files(&up), ascending, "{key:?} ascending");
        let down = rows(&sample, &mode(false, LinesShown::Off, Some((key, true))));
        let descending: Vec<u32> = ascending.iter().rev().copied().collect();
        assert_eq!(files(&down), descending, "{key:?} descending");
    }
}

#[test]
fn grouped_sort_orders_files_inside_each_folder() {
    let built = rows(
        &sample(),
        &mode(true, LinesShown::Off, Some((SortKey::Name, false))),
    );
    assert_eq!(
        built,
        vec![
            Row::Folder(1),
            Row::File(1),
            Row::Folder(0),
            Row::File(0),
            Row::File(2),
        ]
    );
}

#[test]
fn ties_keep_discovery_order() {
    let tied = store(vec![
        found("c.rs", 5, 0, 0),
        found("a.rs", 5, 0, 0),
        found("b.rs", 5, 0, 0),
    ]);
    let built = rows(
        &tied,
        &mode(false, LinesShown::Off, Some((SortKey::Size, true))),
    );
    assert_eq!(files(&built), vec![0, 1, 2]);
}

#[test]
fn index_of_and_nearest_find_rows_and_fall_back_to_the_file() {
    let sample = sample();
    let on = rows(&sample, &mode(false, LinesShown::On, None));
    let line = Row::Line { file: 0, nth: 2 };
    assert_eq!(index_of(&on, line), Some(3));
    assert_eq!(nearest(&on, line), 3);

    let off = rows(&sample, &mode(false, LinesShown::Off, None));
    assert_eq!(index_of(&off, line), None);
    assert_eq!(nearest(&off, line), 0);
    assert_eq!(nearest(&off, Row::More { file: 2, hidden: 1 }), 2);
    assert_eq!(nearest(&off, Row::Folder(1)), 0);
    assert_eq!(nearest(&[], Row::File(4)), 0);
}
