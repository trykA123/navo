use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::fake::{FakeSource, ReadGate, ReadTracking, SlowSpec};
use super::{batches_with_row, collect, start};
use crate::content::CONTENT_BLOCK;
use crate::model::{Reason, Status};
use crate::queue::{LARGE_FILE, worker_limit};
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
    let workers = worker_limit().min(4);
    for index in 0..workers {
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
    while total.load(Ordering::Acquire) < u64::try_from(workers).unwrap() {
        assert!(Instant::now() < deadline, "workers did not start reading");
        thread::sleep(Duration::from_millis(1));
    }

    marker.store(true, Ordering::Release);
    session.cancel();
    drop(session);
    assert!(
        after.load(Ordering::Acquire) <= u64::try_from(workers).unwrap(),
        "{} blocks were read after cancel",
        after.load(Ordering::Acquire)
    );
}

#[test]
fn size_time_mode_reads_nothing() {
    let marker = Arc::new(AtomicBool::new(false));
    let total = Arc::new(AtomicU64::new(0));
    let after = Arc::new(AtomicU64::new(0));
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("a.bin", b"data".to_vec());
    right.add_file("a.bin", b"data".to_vec());
    left.track_reads(ReadTracking {
        delay: Duration::ZERO,
        marker: Arc::clone(&marker),
        total: Arc::clone(&total),
        after: Arc::clone(&after),
    });
    right.track_reads(ReadTracking {
        delay: Duration::ZERO,
        marker,
        total: Arc::clone(&total),
        after,
    });
    let session = start(left, right, CheckMode::SizeTime);
    let batches = collect(&session);

    assert_eq!(total.load(Ordering::Acquire), 0);
    let final_batch = batches.last().expect("final batch");
    assert_eq!(final_batch.totals.same_size_time, 1);
}

#[test]
fn stopping_one_row_ends_it_not_checked() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("a.bin", vec![0xAA; 4 * 1024 * 1024]);
    right.add_file("a.bin", vec![0xAA; 4 * 1024 * 1024]);
    left.set_open_delay(Duration::from_secs(2));
    right.set_open_delay(Duration::from_secs(2));
    let session = start(left, right, CheckMode::Content);
    let first = session
        .batches()
        .recv_timeout(Duration::from_secs(20))
        .expect("initial batch");
    assert_eq!(first.rows[0].raw_status, Status::Checking);
    let id = first.rows[0].id;
    assert!(session.stop_row(id));

    let batches = collect(&session);
    let final_batch = batches.last().expect("final batch");
    assert_eq!(final_batch.totals.not_checked, 1);
    let row = batches_with_row(&batches, "a.bin").last().expect("row");
    assert_eq!(row.1.id, id);
    assert_eq!(row.1.raw_status, Status::NotChecked(Reason::Stopped));
}

#[test]
fn stopping_all_rows_ends_them_not_checked() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for index in 0..8 {
        let name = format!("a{index}.bin");
        left.add_file(&name, vec![0xAA; 4 * 1024 * 1024]);
        right.add_file(&name, vec![0xAA; 4 * 1024 * 1024]);
    }
    left.set_open_delay(Duration::from_secs(2));
    right.set_open_delay(Duration::from_secs(2));
    let session = start(left, right, CheckMode::Content);
    let first = session
        .batches()
        .recv_timeout(Duration::from_secs(20))
        .expect("initial batch");
    assert_eq!(first.totals.checking, 8);
    session.stop_all();

    let batches = collect(&session);
    let final_batch = batches.last().expect("final batch");
    assert_eq!(final_batch.totals.checking, 0);
    assert_eq!(final_batch.totals.not_checked, 8);
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

#[test]
fn large_row_progress_reaches_the_consumer() {
    let size = LARGE_FILE + 1;
    let marker = Arc::new(AtomicBool::new(false));
    let total = Arc::new(AtomicU64::new(0));
    let after = Arc::new(AtomicU64::new(0));
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    let bytes = vec![0xAA; usize::try_from(size).unwrap()];
    left.add_file("big.bin", bytes.clone());
    right.add_file("big.bin", bytes);
    for source in [&mut left, &mut right] {
        source.track_reads(ReadTracking {
            delay: Duration::from_millis(10),
            marker: Arc::clone(&marker),
            total: Arc::clone(&total),
            after: Arc::clone(&after),
        });
    }
    let session = start(left, right, CheckMode::Content);
    let batches = collect(&session);

    let id = batches_with_row(&batches, "big.bin")
        .next()
        .expect("big row")
        .1
        .id;
    let progress: Vec<_> = batches
        .iter()
        .flat_map(|batch| batch.progress.iter())
        .filter(|progress| progress.id == id)
        .collect();
    assert!(!progress.is_empty(), "big row never reported progress");
    assert!(
        progress
            .iter()
            .any(|progress| progress.bytes_done > 0 && progress.bytes_total == size)
    );
}

fn stop_while_reading(right_byte: u8) {
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let gate = ReadGate {
        started: Arc::clone(&started),
        release: Arc::clone(&release),
    };
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("a.bin", vec![0xAA; CONTENT_BLOCK]);
    right.add_file("a.bin", vec![right_byte; CONTENT_BLOCK]);
    left.block_first_read(gate.clone());
    right.block_first_read(gate);
    let session = start(left, right, CheckMode::Content);
    let first = session
        .batches()
        .recv_timeout(Duration::from_secs(20))
        .expect("initial batch");
    let id = first.rows[0].id;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !started.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "content read did not start");
        thread::yield_now();
    }
    assert!(session.stop_row(id));
    release.store(true, Ordering::Release);

    let batches = collect(&session);
    let final_batch = batches.last().expect("final batch");
    assert_eq!(final_batch.totals.not_checked, 1);
    assert_eq!(final_batch.totals.same, 0);
    assert_eq!(final_batch.totals.different, 0);
    let row = batches_with_row(&batches, "a.bin").last().expect("row");
    assert_eq!(row.1.id, id);
    assert_eq!(row.1.raw_status, Status::NotChecked(Reason::Stopped));
}

#[test]
fn stopping_while_a_differing_block_is_read_settles_not_checked() {
    stop_while_reading(0xBB);
}

#[test]
fn stopping_while_the_final_equal_block_is_read_settles_not_checked() {
    stop_while_reading(0xAA);
}
