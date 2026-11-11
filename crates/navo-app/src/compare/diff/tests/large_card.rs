//! Tests for the large card: hunk sorting, fullness, detail streaming and cancel.

use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use navo_compare::model::CancelToken;
use navo_ui::compare_diff::{CardDetail, CardScan, LargeHunk, LargeProgress, LargeStage};

use crate::compare::diff::cards::{large_card, run};
use crate::compare::diff::tests::large::{WAIT, pair, request};

/// One one-line hunk at the given left and right line.
fn hunk(left: u64, right: u64) -> LargeHunk {
    LargeHunk {
        left_rows: left..left + 1,
        right_rows: right..right + 1,
        left_bytes: 0..1,
        right_bytes: 0..1,
    }
}

/// `count` one-line hunks at the same line on both sides.
fn hunks(count: u64) -> Vec<LargeHunk> {
    (0..count).map(|at| hunk(at, at)).collect()
}

#[test]
fn large_card_sorts_and_keeps_six_hunks() {
    let found: Vec<LargeHunk> = [5, 0, 7, 2, 4, 1, 6, 3]
        .into_iter()
        .map(|at| hunk(at, at))
        .collect();
    let card = large_card(&found, None);
    let starts: Vec<u64> = card.hunks.iter().map(|hunk| hunk.left_rows.start).collect();
    assert_eq!(starts, vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(card.found, 8);
    assert_eq!(card.scan, CardScan::Full);
}

#[test]
fn large_card_is_full_at_six_hunks_and_done_when_finished() {
    assert_eq!(large_card(&hunks(5), None).scan, CardScan::Running);
    assert_eq!(large_card(&hunks(6), None).scan, CardScan::Full);
    let done = LargeProgress {
        stage: LargeStage::Sections,
        done: 2,
        total: 2,
    };
    let card = large_card(&hunks(6), Some(done));
    assert_eq!(card.scan, CardScan::Done);
    assert_eq!(card.found, 6);
}

#[test]
fn a_large_pair_streams_details_and_ends_with_a_last_one() {
    let dir = tempfile::tempdir().unwrap();
    let input = pair(dir.path(), 4000, 1234);
    let seen = Mutex::new(Vec::new());
    let emit = |detail: CardDetail, last: bool| {
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((detail, last));
    };
    run(&request(), input, &CancelToken::new(), &emit);
    let seen = seen.into_inner().unwrap_or_else(PoisonError::into_inner);
    let (last_detail, last) = seen.last().expect("a large detail arrived");
    assert!(*last, "the last snapshot was not final");
    assert!(matches!(last_detail, CardDetail::Large(card) if card.scan == CardScan::Done));
}

#[test]
fn a_cancelled_large_job_returns_without_a_last_detail() {
    let dir = tempfile::tempdir().unwrap();
    let input = pair(dir.path(), 4000, 1234);
    let cancel = CancelToken::new();
    cancel.cancel();
    let seen = Mutex::new(Vec::new());
    let emit = |detail: CardDetail, last: bool| {
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((detail, last));
    };
    let started = Instant::now();
    run(&request(), input, &cancel, &emit);
    assert!(started.elapsed() < WAIT);
    assert!(
        seen.into_inner()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    );
}
