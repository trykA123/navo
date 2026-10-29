use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::super::run::MAX_WORKERS;
use super::fake::{FakeSource, ReadTracking, SlowSpec};
use super::{batches_with_row, collect, start};
use crate::model::{Reason, Status};
use crate::session::CheckMode;

#[test]
fn refresh_stops_a_slow_listing_quickly() {
    let started = Arc::new(AtomicBool::new(false));
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for source in [&mut left, &mut right] {
        source.add_slow_folder(
            "slow",
            SlowSpec {
                count: 200_000,
                every: 1_000,
                delay: Duration::from_millis(1),
                started: Arc::clone(&started),
            },
        );
    }
    let mut session = start(left, right, CheckMode::SizeTime);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !started.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "slow listing did not start");
        thread::sleep(Duration::from_millis(1));
    }

    let before = Instant::now();
    session.refresh();
    assert!(
        before.elapsed() < Duration::from_millis(200),
        "refresh took {:?}",
        before.elapsed()
    );
}

#[test]
fn cancel_stops_workers_after_one_block() {
    let marker = Arc::new(AtomicBool::new(false));
    let total = Arc::new(AtomicU64::new(0));
    let after = Arc::new(AtomicU64::new(0));
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for index in 0..MAX_WORKERS {
        let name = format!("big{index}.bin");
        left.add_file(&name, vec![0xAA; 4 * 1024 * 1024]);
        right.add_file(&name, vec![0xAA; 4 * 1024 * 1024]);
    }
    left.track_reads(ReadTracking {
        delay: Duration::from_millis(10),
        marker: Arc::clone(&marker),
        total: Arc::clone(&total),
        after: Arc::clone(&after),
    });
    right.track_reads(ReadTracking {
        delay: Duration::from_millis(10),
        marker: Arc::clone(&marker),
        total: Arc::clone(&total),
        after: Arc::clone(&after),
    });
    let session = start(left, right, CheckMode::Content);
    let deadline = Instant::now() + Duration::from_secs(5);
    while total.load(Ordering::Acquire) < u64::try_from(MAX_WORKERS).unwrap() {
        assert!(Instant::now() < deadline, "workers did not start reading");
        thread::sleep(Duration::from_millis(1));
    }

    marker.store(true, Ordering::Release);
    session.cancel();
    drop(session);
    assert!(
        after.load(Ordering::Acquire) <= u64::try_from(MAX_WORKERS).unwrap(),
        "{} blocks were read after cancel",
        after.load(Ordering::Acquire)
    );
}

#[test]
fn a_panicked_content_worker_finishes_its_row() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("a.bin", b"data".to_vec());
    right.add_file("a.bin", b"data".to_vec());
    left.panic_on_next_open();
    let session = start(left, right, CheckMode::Content);
    let batches = collect(&session);

    let final_batch = batches.last().expect("final batch");
    assert!(final_batch.is_final);
    assert_eq!(final_batch.totals.checking, 0);
    assert_eq!(final_batch.totals.unavailable, 1);
    let row = batches_with_row(&batches, "a.bin")
        .last()
        .expect("row after panic");
    assert_eq!(
        row.1.raw_status,
        Status::Unavailable(Reason::WorkerPanicked)
    );
}

#[test]
fn a_panicked_walker_finishes_with_a_problem() {
    let left = FakeSource::new(true);
    let right = FakeSource::new(true);
    left.panic_on_next_list();
    let session = start(left, right, CheckMode::SizeTime);
    let batches = collect(&session);

    let final_batch = batches.last().expect("final batch");
    assert!(final_batch.is_final);
    assert_eq!(final_batch.problem, Some(Reason::WorkerPanicked));
    assert_eq!(final_batch.totals.checking, 0);
}

#[test]
fn a_folder_listing_error_updates_the_parent_row_once() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for source in [&mut left, &mut right] {
        source.add_folder("sub");
    }
    left.fail_listing("sub", Reason::ReadError);
    let session = start(left, right, CheckMode::SizeTime);
    let batches = collect(&session);

    let final_batch = batches.last().expect("final batch");
    assert_eq!(final_batch.totals.rows, 1);
    assert_eq!(final_batch.totals.same, 0);
    assert_eq!(final_batch.totals.unavailable, 1);
    let sub = batches_with_row(&batches, "sub").last().expect("sub row");
    assert_eq!(sub.1.raw_status, Status::Unavailable(Reason::ReadError));
}
