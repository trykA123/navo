//! Tests for the binary card: the first region's window, clamping, equal files and row rounding.

use std::fs;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use navo_compare::bytes::Align;
use navo_compare::model::CancelToken;
use navo_ui::compare_diff::{BytesCard, CardDetail, CardScan, PairRequest};

use crate::compare::diff::cards::{BytesJob, run_bytes, window_start};
use crate::compare::diff::sources::Sources;
use crate::compare::settings::CompareSettings;

/// A request whose two locations are the real test files the window reads.
fn card_request(root: &Path, left: &str, right: &str) -> PairRequest {
    PairRequest {
        id: 31,
        path: "pair.bin".to_owned(),
        left: Some(super::location(&root.join(left))),
        right: Some(super::location(&root.join(right))),
    }
}

/// A job over the two named files, aligned as the engine would for a binary pair.
fn job(root: &Path, left: &str, right: &str, align: Align) -> BytesJob {
    BytesJob {
        request: card_request(root, left, right),
        input: super::input(root, left, right, align),
        sources: Sources::without_spool(),
        settings: CompareSettings::default(),
    }
}

/// Run one job and return every emitted detail with its final flag.
fn details(job: BytesJob) -> Vec<(CardDetail, bool)> {
    let seen = Mutex::new(Vec::new());
    let emit = |detail: CardDetail, last: bool| {
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((detail, last));
    };
    run_bytes(job, &CancelToken::new(), &emit);
    seen.into_inner().unwrap_or_else(PoisonError::into_inner)
}

/// The last card detail, which must be final and binary.
fn last_card(seen: Vec<(CardDetail, bool)>) -> BytesCard {
    let (detail, last) = seen.last().expect("a detail arrived");
    assert!(*last, "the last detail was not final");
    match detail {
        CardDetail::Bytes(card) => card.clone(),
        other => panic!("unexpected detail {other:?}"),
    }
}

#[test]
fn the_first_region_window_holds_four_rows_each_side() {
    let dir = tempfile::tempdir().unwrap();
    let left = vec![b'a'; 300];
    let mut right = left.clone();
    right[100] = b'b';
    fs::write(dir.path().join("left.bin"), &left).unwrap();
    fs::write(dir.path().join("right.bin"), &right).unwrap();

    let card = last_card(details(job(
        dir.path(),
        "left.bin",
        "right.bin",
        Align::Offset,
    )));
    let window = card.window.expect("a window");
    assert_eq!(card.scan, CardScan::Full);
    assert_eq!(window.left_start, 32);
    assert_eq!(window.right_start, 32);
    assert_eq!(window.left, left[32..176]);
    assert_eq!(window.right, right[32..176]);
}

#[test]
fn a_region_near_the_start_clamps_the_window_at_zero() {
    let dir = tempfile::tempdir().unwrap();
    let left = vec![b'a'; 40];
    let mut right = left.clone();
    right[10] = b'b';
    fs::write(dir.path().join("left.bin"), &left).unwrap();
    fs::write(dir.path().join("right.bin"), &right).unwrap();

    let card = last_card(details(job(
        dir.path(),
        "left.bin",
        "right.bin",
        Align::Offset,
    )));
    let window = card.window.expect("a window");
    assert_eq!(window.left_start, 0);
    assert_eq!(window.right_start, 0);
    assert_eq!(window.left, left);
    assert_eq!(window.right, right);
}

#[test]
fn equal_files_end_done_without_a_window() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = vec![b'a'; 100];
    fs::write(dir.path().join("left.bin"), &bytes).unwrap();
    fs::write(dir.path().join("right.bin"), &bytes).unwrap();

    let card = last_card(details(job(
        dir.path(),
        "left.bin",
        "right.bin",
        Align::Offset,
    )));
    assert_eq!(card.scan, CardScan::Done);
    assert!(card.window.is_none());
}

#[test]
fn window_start_rounds_down_to_a_row() {
    assert_eq!(window_start(0), 0);
    assert_eq!(window_start(15), 0);
    assert_eq!(window_start(16), 0);
    assert_eq!(window_start(79), 0);
    assert_eq!(window_start(80), 16);
    assert_eq!(window_start(100), 32);
}
