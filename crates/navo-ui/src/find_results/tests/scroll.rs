//! Whole-row scroll and sticky folder band tests for the find results window.

use navo_core::find::{FileHits, LineHit};
use navo_core::{EntryKind, ProviderPath};
use navo_fs::find::{FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use crate::find_results::rows::{LinesShown, Row, RowMode, rows};
use crate::find_results::scroll::{Scroll, WHEEL_ROWS};
use crate::find_results::store::HitStore;

fn found(relative: &str, lines: u64) -> FoundFile {
    FoundFile {
        relative: ProviderPath::from_text(relative),
        entry: FsEntry {
            name: ProviderPath::from_text(relative),
            kind: EntryKind::File,
            size: 10,
            modified_ms: None,
            attributes: Attributes::default(),
        },
        hits: FileHits {
            hits: (1..=lines)
                .map(|line| LineHit {
                    line,
                    column: 0,
                    text: "match".into(),
                })
                .collect(),
            total: u32::try_from(lines).unwrap_or(u32::MAX),
            skipped: None,
        },
    }
}

/// Two folders: folder 0 (`a`) has file 0 (4 hits -> 3 lines + More) and file 1 (1 hit);
/// folder 1 (`b`) has file 2 (2 hits).
///
/// Grouped rows with lines on (11 rows):
/// 0: Folder(0), 1: File(0), 2: Line(0,0), 3: Line(0,1), 4: Line(0,2), 5: More(0,1),
/// 6: File(1), 7: Line(1,0), 8: Folder(1), 9: File(2), 10: Line(2,0), 11: Line(2,1) -> 12 rows.
fn sample() -> (HitStore, Vec<Row>) {
    let mut store = HitStore::default();
    store
        .push(
            0,
            &FoundBatch {
                files: vec![
                    found("a/first.rs", 4),
                    found("a/second.rs", 1),
                    found("b/third.rs", 2),
                ],
                progress: FindProgress::default(),
            },
        )
        .expect("a small batch fits");
    let built = rows(
        &store,
        &RowMode {
            grouped: true,
            lines: LinesShown::On,
            sort: None,
        },
    );
    (store, built)
}

#[test]
fn wheel_moves_three_rows_per_notch_and_clamps() {
    assert_eq!(WHEEL_ROWS, 3);
    let mut scroll = Scroll { top: 0, visible: 5 };
    scroll.wheel(1, 20);
    assert_eq!(scroll.top, 3);
    scroll.wheel(2, 20);
    assert_eq!(scroll.top, 9);
    scroll.wheel(10, 20);
    assert_eq!(scroll.top, 15);
    scroll.wheel(-1, 20);
    assert_eq!(scroll.top, 12);
    scroll.wheel(-10, 20);
    assert_eq!(scroll.top, 0);
}

#[test]
fn wheel_handles_short_lists_zero_visible_and_extreme_notches() {
    let mut short = Scroll { top: 4, visible: 8 };
    short.wheel(1, 5);
    assert_eq!(short.top, 0);
    short.wheel(1, 0);
    assert_eq!(short.top, 0);

    let mut extreme = Scroll { top: 2, visible: 4 };
    extreme.wheel(i32::MAX, 10);
    assert_eq!(extreme.top, 6);
    extreme.wheel(i32::MIN, 10);
    assert_eq!(extreme.top, 0);

    let mut unmeasured = Scroll { top: 0, visible: 0 };
    unmeasured.wheel(2, 10);
    assert_eq!(unmeasured.top, 6);
    unmeasured.wheel(10, 10);
    assert_eq!(unmeasured.top, 9);
}

#[test]
fn band_shows_the_top_rows_folder_unless_top_is_a_folder() {
    let (store, built) = sample();
    assert_eq!(built.len(), 12);

    assert_eq!(Scroll { top: 0, visible: 5 }.band(&built, &store), None);
    assert_eq!(Scroll { top: 1, visible: 5 }.band(&built, &store), Some(0));
    assert_eq!(Scroll { top: 2, visible: 5 }.band(&built, &store), Some(0));
    assert_eq!(Scroll { top: 5, visible: 5 }.band(&built, &store), Some(0));
    assert_eq!(Scroll { top: 8, visible: 5 }.band(&built, &store), None);
    assert_eq!(Scroll { top: 9, visible: 5 }.band(&built, &store), Some(1));
    assert_eq!(
        Scroll {
            top: 11,
            visible: 5
        }
        .band(&built, &store),
        Some(1)
    );
    assert_eq!(
        Scroll {
            top: 99,
            visible: 5
        }
        .band(&built, &store),
        None
    );
    assert_eq!(Scroll { top: 0, visible: 5 }.band(&[], &store), None);
}

#[test]
fn keep_visible_uncovers_cursor_behind_band_and_scrolls_down_past_bottom() {
    let (store, built) = sample();

    // Top is row 2 (Line in folder 0) so the band covers row 2; visible uncovered rows are 3..=6.
    let mut scroll = Scroll { top: 2, visible: 5 };
    scroll.keep_visible(3, &built, &store);
    assert_eq!(scroll.top, 2);
    scroll.keep_visible(6, &built, &store);
    assert_eq!(scroll.top, 2);

    // Cursor moves below the last full visible row (6 -> 7): top scrolls down to 3.
    scroll.keep_visible(7, &built, &store);
    assert_eq!(scroll.top, 3);

    // Cursor moves into the band slot at top (row 3, a Line row): top steps up to 2.
    scroll.keep_visible(3, &built, &store);
    assert_eq!(scroll.top, 2);

    // Cursor moves up to row 1 (File(0)): top becomes 0 (Folder(0)), revealing row 1 below it.
    scroll.keep_visible(1, &built, &store);
    assert_eq!(scroll.top, 0);

    // Cursor at row 0 (Folder(0)): band is hidden, so top stays 0.
    scroll.keep_visible(0, &built, &store);
    assert_eq!(scroll.top, 0);
}

#[test]
fn keep_visible_does_not_step_above_a_folder_row_at_top() {
    let (store, built) = sample();
    // Row 8 is Folder(1); scrolling up onto it needs no extra room for a band.
    let mut scroll = Scroll { top: 9, visible: 4 };
    scroll.keep_visible(8, &built, &store);
    assert_eq!(scroll.top, 8);

    // Row 9 is File(2); scrolling up onto it steps top to 8 (Folder(1)) so it clears the band.
    let mut from_below = Scroll {
        top: 10,
        visible: 4,
    };
    from_below.keep_visible(9, &built, &store);
    assert_eq!(from_below.top, 8);
}

#[test]
fn keep_visible_handles_edge_viewports_empty_and_flat_lists() {
    let (store, built) = sample();

    let mut empty = Scroll { top: 4, visible: 5 };
    empty.keep_visible(0, &[], &store);
    assert_eq!(empty.top, 0);

    // A 1-row viewport cannot leave room for a band without pushing the cursor off screen.
    let mut one_row = Scroll { top: 0, visible: 1 };
    one_row.keep_visible(3, &built, &store);
    assert_eq!(one_row.top, 3);

    // Unmeasured viewport (visible == 0) only clamps stale top into bounds.
    let mut zero_vis = Scroll {
        top: 50,
        visible: 0,
    };
    zero_vis.keep_visible(2, &built, &store);
    assert_eq!(zero_vis.top, built.len() - 1);

    // Flat list where row 0 is a File: cursor at 0 saturates cleanly at top 0.
    let flat = rows(
        &store,
        &RowMode {
            grouped: false,
            lines: LinesShown::Off,
            sort: None,
        },
    );
    let mut flat_scroll = Scroll { top: 2, visible: 2 };
    flat_scroll.keep_visible(0, &flat, &store);
    assert_eq!(flat_scroll.top, 0);
}

#[test]
fn snap_maps_fraction_to_whole_row_indices_safely() {
    assert_eq!(Scroll::snap(0.0, 0), 0);
    assert_eq!(Scroll::snap(1.0, 0), 0);
    assert_eq!(Scroll::snap(0.5, 1), 0);
    assert_eq!(Scroll::snap(0.0, 10), 0);
    assert_eq!(Scroll::snap(1.0, 10), 9);
    assert_eq!(Scroll::snap(0.5, 11), 5);
    assert_eq!(Scroll::snap(0.24, 11), 2);
    assert_eq!(Scroll::snap(0.26, 11), 3);
    assert_eq!(Scroll::snap(-0.5, 10), 0);
    assert_eq!(Scroll::snap(1.5, 10), 9);
    assert_eq!(Scroll::snap(f32::NAN, 10), 0);
    assert_eq!(Scroll::snap(f32::NEG_INFINITY, 10), 0);
    assert_eq!(Scroll::snap(f32::INFINITY, 10), 9);
}
