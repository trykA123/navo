use proptest::prelude::*;
use rusty_automotive::{Layout, LayoutOptions, Row};
use std::ops::Range;

fn build(ranges: &[Range<u64>]) -> Layout {
    Layout::new(ranges, LayoutOptions::default())
}

fn data(address: u64) -> Option<Row> {
    Some(Row::Data {
        address,
        missing: Vec::new(),
    })
}

fn span(start: u64, end: u64) -> Vec<Range<u64>> {
    std::iter::once(start..end).collect()
}

fn sized(ranges: &[Range<u64>], row_bytes: u64, fold_under: u64) -> Layout {
    Layout::new(
        ranges,
        LayoutOptions {
            row_bytes,
            fold_under,
        },
    )
}

#[test]
fn a_single_range_fills_aligned_rows() {
    let layout = build(std::slice::from_ref(&(0x105..0x121)));
    assert_eq!(layout.len(), 3);
    assert_eq!(layout.row(0), data(0x100));
    assert_eq!(layout.row(2), data(0x120));
    assert_eq!(layout.row(3), None);
}

#[test]
fn a_gap_wider_than_a_row_folds_into_one_row() {
    let layout = build(&[0x0..0x20, 0x1000..0x1010]);
    assert_eq!(layout.len(), 4);
    assert_eq!(layout.row(1), data(0x10));
    assert_eq!(
        layout.row(2),
        Some(Row::Gap {
            range: 0x20..0x1000
        })
    );
    assert_eq!(layout.row(3), data(0x1000));
}

#[test]
fn a_gap_inside_one_row_stays_in_the_row() {
    let layout = build(&[0x0..0x4, 0x8..0x14]);
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.row(1), data(0x10));
}

#[test]
fn ranges_that_share_a_row_across_a_short_gap_stay_together() {
    let layout = build(&[0x0..0x18, 0x1C..0x30]);
    assert_eq!(layout.len(), 3);
    assert!(
        (0..3).all(|row| matches!(layout.row(row), Some(Row::Data { .. }))),
        "no gap row expected"
    );
}

#[test]
fn an_address_finds_its_row() {
    let layout = build(&[0x0..0x20, 0x1000..0x1010]);
    assert_eq!(layout.row_of(0x1F), Some(1));
    assert_eq!(layout.row_of(0x500), Some(2));
    assert_eq!(layout.row_of(0x1008), Some(3));
    assert_eq!(layout.row_of(0x1010), None);
}

#[test]
fn an_empty_image_has_no_rows() {
    let layout = build(&[]);
    assert_eq!(layout.len(), 0);
    assert_eq!(layout.row(0), None);
    assert_eq!(layout.row_of(0), None);
}

#[test]
fn rows_reach_the_top_of_the_address_space() {
    let layout = build(std::slice::from_ref(&(0xFFFF_FFF0..0x1_0000_0000)));
    assert_eq!(layout.len(), 1);
    assert_eq!(layout.row_of(0xFFFF_FFFF), Some(0));
}

#[test]
fn visible_rows_become_one_address_window_per_block() {
    let layout = build(&[0x0..0x20, 0x1000..0x1010]);
    assert_eq!(layout.windows(0..4), vec![0x0..0x20, 0x1000..0x1010]);
    assert_eq!(layout.windows(1..2), vec![0x10..0x20]);
    assert_eq!(layout.windows(2..3), Vec::<std::ops::Range<u64>>::new());
}

#[test]
fn short_gaps_fold_into_the_rows_around_them() {
    let layout = build(&[0x0..0x18, 0x20..0x30]);
    assert_eq!(layout.len(), 3);
    assert_eq!(layout.folded(), 1);
    assert_eq!(
        layout.row(1),
        Some(Row::Data {
            address: 0x10,
            missing: span(0x18, 0x20),
        })
    );
    assert_eq!(layout.row(0), data(0x0));
    assert_eq!(layout.row(2), data(0x20));
}

#[test]
fn a_folded_gap_spanning_rows_is_clipped_per_row() {
    let layout = build(&[0x0..0x8, 0x28..0x30]);
    assert_eq!(layout.len(), 3);
    let missing = |row| match layout.row(row) {
        Some(Row::Data { missing, .. }) => missing,
        other => panic!("expected data, got {other:?}"),
    };
    assert_eq!(missing(0), vec![0x8..0x10]);
    assert_eq!(missing(1), vec![0x10..0x20]);
    assert_eq!(missing(2), vec![0x20..0x28]);
    assert_eq!(layout.windows(0..3), vec![0x0..0x30]);
}

#[test]
fn many_ranges_split_by_padding_fold_into_two_blocks() {
    let mut ranges: Vec<Range<u64>> = (0..99).map(|at| at * 0x20..at * 0x20 + 0x10).collect();
    ranges.extend((0..99).map(|at| 0x10_0000 + at * 0x20..0x10_0000 + at * 0x20 + 0x10));
    assert_eq!(ranges.len(), 198);
    let layout = build(&ranges);
    assert_eq!(layout.folded(), 196);
    assert_eq!(layout.len(), 197 + 1 + 197);
    let gaps = (0..layout.len())
        .filter(|&row| matches!(layout.row(row), Some(Row::Gap { .. })))
        .count();
    assert_eq!(gaps, 1);
}

#[test]
fn a_gap_of_300_bytes_stays_a_gap_row() {
    let layout = build(&[0x0..0x10, 0x13C..0x150]);
    assert_eq!(layout.folded(), 0);
    assert_eq!(layout.row(0), data(0x0));
    assert_eq!(layout.row(1), Some(Row::Gap { range: 0x10..0x13C }));
    assert_eq!(layout.row(2), data(0x130));
}

#[test]
fn a_gap_just_under_the_limit_folds_and_the_limit_does_not() {
    assert_eq!(build(&[0x0..0x10, 0x10F..0x120]).folded(), 1);
    assert_eq!(build(&[0x0..0x10, 0x110..0x120]).folded(), 0);
}

#[test]
fn rows_can_be_32_bytes_wide() {
    let layout = sized(std::slice::from_ref(&(0x105..0x15F)), 32, 256);
    assert_eq!(layout.row_bytes(), 32);
    assert_eq!(layout.len(), 3);
    assert_eq!(layout.row(0), data(0x100));
    assert_eq!(layout.row(2), data(0x140));
    assert_eq!(layout.row_of(0x15E), Some(2));
    assert_eq!(layout.windows(0..3), vec![0x100..0x160]);
}

#[test]
fn folding_a_gap_removes_its_gap_row_at_both_widths() {
    let ranges = [0x0..0x10, 0x30..0x40];
    assert_eq!(sized(&ranges, 16, 0).len(), 3);
    assert_eq!(sized(&ranges, 16, 256).len(), 4);
    let wide = sized(&ranges, 32, 256);
    assert_eq!(wide.len(), 2);
    assert_eq!(
        wide.row(0),
        Some(Row::Data {
            address: 0x0,
            missing: span(0x10, 0x20),
        })
    );
    assert_eq!(
        wide.row(1),
        Some(Row::Data {
            address: 0x20,
            missing: span(0x20, 0x30),
        })
    );
}

#[test]
fn without_folding_a_short_gap_is_not_marked() {
    let layout = sized(&[0x0..0x4, 0x8..0x14], 16, 0);
    assert_eq!(layout.folded(), 0);
    assert_eq!(layout.row(0), data(0x0));
}

#[test]
fn the_default_options_are_16_bytes_and_256() {
    let options = LayoutOptions::default();
    assert_eq!(options.row_bytes, 16);
    assert_eq!(options.fold_under, 256);
    assert_eq!(Layout::default().row_bytes(), 16);
}

fn ranges() -> impl Strategy<Value = Vec<Range<u64>>> {
    prop::collection::vec((1u64..600, 1u64..80), 1..12).prop_map(|steps| {
        let mut at = 0x1000;
        steps
            .into_iter()
            .map(|(gap, len)| {
                let range = at + gap..at + gap + len;
                at = range.end;
                range
            })
            .collect()
    })
}

proptest! {
    #[test]
    fn row_of_and_windows_agree(
        ranges in ranges(),
        wide in any::<bool>(),
        fold_under in prop::sample::select(vec![0u64, 16, 64, 256]),
    ) {
        let row_bytes = if wide { 32 } else { 16 };
        let layout = sized(&ranges, row_bytes, fold_under);
        for range in &ranges {
            for address in [range.start, range.end - 1, range.start + (range.end - range.start) / 2] {
                let row = layout.row_of(address).expect("an address in a range has a row");
                let Some(Row::Data { address: first, missing }) = layout.row(row) else {
                    panic!("row {row} of {address:#x} is not data");
                };
                prop_assert!(first <= address && address < first + row_bytes);
                prop_assert!(!missing.iter().any(|gap| gap.contains(&address)));
                let covered = layout.windows(row..row + 1);
                prop_assert_eq!(covered, vec![first..first + row_bytes]);
            }
        }
    }

    #[test]
    fn folded_bytes_lie_between_ranges(
        ranges in ranges(),
        wide in any::<bool>(),
    ) {
        let row_bytes = if wide { 32 } else { 16 };
        let layout = sized(&ranges, row_bytes, 256);
        for row in 0..layout.len() {
            if let Some(Row::Data { address, missing }) = layout.row(row) {
                for gap in missing {
                    prop_assert!(gap.start >= address && gap.end <= address + row_bytes);
                    prop_assert!(!ranges.iter().any(|range| range.start < gap.end && gap.start < range.end));
                }
            }
        }
    }
}
