use rusty_automotive::{Layout, Row};

#[test]
fn a_single_range_fills_aligned_rows() {
    let layout = Layout::new(std::slice::from_ref(&(0x105..0x121)));
    assert_eq!(layout.len(), 3);
    assert_eq!(layout.row(0), Some(Row::Data { address: 0x100 }));
    assert_eq!(layout.row(2), Some(Row::Data { address: 0x120 }));
    assert_eq!(layout.row(3), None);
}

#[test]
fn a_gap_wider_than_a_row_folds_into_one_row() {
    let layout = Layout::new(&[0x0..0x20, 0x1000..0x1010]);
    assert_eq!(layout.len(), 4);
    assert_eq!(layout.row(1), Some(Row::Data { address: 0x10 }));
    assert_eq!(
        layout.row(2),
        Some(Row::Gap {
            range: 0x20..0x1000
        })
    );
    assert_eq!(layout.row(3), Some(Row::Data { address: 0x1000 }));
}

#[test]
fn a_gap_inside_one_row_stays_in_the_row() {
    let layout = Layout::new(&[0x0..0x4, 0x8..0x14]);
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.row(1), Some(Row::Data { address: 0x10 }));
}

#[test]
fn ranges_that_share_a_row_across_a_short_gap_stay_together() {
    let layout = Layout::new(&[0x0..0x18, 0x1C..0x30]);
    assert_eq!(layout.len(), 3);
    assert!(
        (0..3).all(|row| matches!(layout.row(row), Some(Row::Data { .. }))),
        "no gap row expected"
    );
}

#[test]
fn an_address_finds_its_row() {
    let layout = Layout::new(&[0x0..0x20, 0x1000..0x1010]);
    assert_eq!(layout.row_of(0x1F), Some(1));
    assert_eq!(layout.row_of(0x500), Some(2));
    assert_eq!(layout.row_of(0x1008), Some(3));
    assert_eq!(layout.row_of(0x1010), None);
}

#[test]
fn an_empty_image_has_no_rows() {
    let layout = Layout::new(&[]);
    assert_eq!(layout.len(), 0);
    assert_eq!(layout.row(0), None);
    assert_eq!(layout.row_of(0), None);
}

#[test]
fn rows_reach_the_top_of_the_address_space() {
    let layout = Layout::new(std::slice::from_ref(&(0xFFFF_FFF0..0x1_0000_0000)));
    assert_eq!(layout.len(), 1);
    assert_eq!(layout.row_of(0xFFFF_FFFF), Some(0));
}

#[test]
fn visible_rows_become_one_address_window_per_block() {
    let layout = Layout::new(&[0x0..0x20, 0x1000..0x1010]);
    assert_eq!(layout.windows(0..4), vec![0x0..0x20, 0x1000..0x1010]);
    assert_eq!(layout.windows(1..2), vec![0x10..0x20]);
    assert_eq!(layout.windows(2..3), Vec::<std::ops::Range<u64>>::new());
}
