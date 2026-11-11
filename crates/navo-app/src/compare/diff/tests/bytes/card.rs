//! Tests for the binary card: the first region's window, clamping, equal files and row rounding.

use std::fs;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use navo_compare::bytes::Align;
use navo_compare::model::CancelToken;
use navo_compare::text::Options;
use navo_ui::compare_diff::{BytesCard, CardDetail, CardScan, PairKind, PairRequest};

use crate::compare::diff::cards::{BytesJob, detail_fn, run_bytes, window_start};
use crate::compare::diff::load::LoadInput;
use crate::compare::diff::pair::EnginePlan;
use crate::compare::diff::sources::Sources;
use crate::compare::diff::stream::DetailJob;
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

/// Run one job with `cancel` and return every emitted detail with its final flag.
fn details_with(job: BytesJob, cancel: &CancelToken) -> Vec<(CardDetail, bool)> {
    let seen = Mutex::new(Vec::new());
    let emit = |detail: CardDetail, last: bool| {
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((detail, last));
    };
    run_bytes(job, cancel, &emit);
    seen.into_inner().unwrap_or_else(PoisonError::into_inner)
}

/// Run one job and return every emitted detail with its final flag.
fn details(job: BytesJob) -> Vec<(CardDetail, bool)> {
    details_with(job, &CancelToken::new())
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

#[test]
fn a_missing_binary_side_gets_a_failed_detail() {
    let seen = Mutex::new(Vec::new());
    let emit = |detail: CardDetail, last: bool| {
        seen.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((detail, last));
    };
    let job = DetailJob {
        id: 41,
        generation: 1,
        request: PairRequest {
            id: 41,
            path: "pair.bin".to_owned(),
            left: None,
            right: None,
        },
        kind: PairKind::Binary,
        cancel: CancelToken::new(),
    };
    let input = LoadInput {
        left: None,
        right: None,
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    detail_fn(EnginePlan::default())(&job, &input, &emit);
    let seen = seen.into_inner().unwrap_or_else(PoisonError::into_inner);
    let (detail, last) = seen.last().expect("a terminal detail");
    assert!(*last, "the failed detail must be final");
    assert_eq!(
        detail,
        &CardDetail::Failed(navo_l10n::tr("compare-could-not-start"))
    );
}

#[test]
fn a_cancelled_binary_job_returns_quickly() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.bin"), b"abcdefghij").unwrap();
    fs::write(dir.path().join("right.bin"), b"abcXefghij").unwrap();
    let mut job = job(dir.path(), "left.bin", "right.bin", Align::Offset);
    job.input.left.1.size = 2_000_000_000;
    job.input.right.1.size = 2_000_000_000;
    let cancel = CancelToken::new();
    cancel.cancel();
    let started = Instant::now();
    let seen = details_with(job, &cancel);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(seen.is_empty(), "a cancelled job sends no detail");
}

#[test]
fn a_side_that_cannot_open_gets_a_failed_detail() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("left.bin"), b"abcdefghij").unwrap();
    fs::write(dir.path().join("right.bin"), b"abcXefghij").unwrap();
    let mut job = job(dir.path(), "left.bin", "right.bin", Align::Offset);
    job.request.left = Some(super::location(&dir.path().join("gone.bin")));
    let seen = details(job);
    let (detail, last) = seen.last().expect("a terminal detail");
    assert!(*last, "the failed detail must be final");
    assert_eq!(
        detail,
        &CardDetail::Failed(navo_l10n::tr("compare-could-not-start"))
    );
}
