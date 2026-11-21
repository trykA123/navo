use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::fake::{FakeSource, SlowSpec};
use super::{collect, start};
use crate::session::{CheckMode, ComparePhase};

#[test]
fn walk_emits_listing_quick_check_and_slow_entries() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_slow_folder(
        "slow",
        SlowSpec {
            count: 4,
            every: 1,
            delay: Duration::from_millis(5),
            started: Arc::new(AtomicBool::new(false)),
        },
    );
    right.add_folder("slow");

    let session = start(left, right, CheckMode::SizeTime);
    session.set_slow_threshold_ms_for_test(1);
    let batches = collect(&session);

    assert!(
        !batches[0].rows.is_empty(),
        "initial rows must arrive in first batch"
    );
    assert!(
        batches
            .iter()
            .any(|batch| matches!(batch.phase, Some(p) if p.phase == ComparePhase::Listing)),
        "expected ComparePhase::Listing batch"
    );
    assert!(
        batches
            .iter()
            .any(|batch| matches!(batch.phase, Some(p) if p.phase == ComparePhase::QuickCheck)),
        "expected ComparePhase::QuickCheck batch"
    );
    assert!(
        batches
            .iter()
            .flat_map(|batch| &batch.slow)
            .any(|entry| entry.rel.as_path().to_text() == "slow" && entry.elapsed_ms >= 1),
        "expected SlowEntry for delayed folder"
    );
}

#[test]
fn content_check_emits_phase_and_slow_entries() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("slow.bin", b"aaaa".to_vec());
    right.add_file("slow.bin", b"bbbb".to_vec());
    left.set_open_delay(Duration::from_millis(5));

    let session = start(left, right, CheckMode::Content);
    session.set_slow_threshold_ms_for_test(1);
    let batches = collect(&session);

    assert!(
        batches.iter().any(|batch| {
            matches!(batch.phase, Some(p) if p.phase == ComparePhase::ContentCheck && p.done >= 1)
        }),
        "expected ComparePhase::ContentCheck batch"
    );
    assert!(
        batches
            .iter()
            .flat_map(|batch| &batch.slow)
            .any(|entry| entry.rel.as_path().to_text() == "slow.bin" && entry.elapsed_ms >= 1),
        "expected SlowEntry for delayed content read"
    );
}

#[test]
fn a_running_content_read_keeps_the_receiver_informed() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("slow.bin", b"aaaa".to_vec());
    right.add_file("slow.bin", b"aaaa".to_vec());
    left.set_open_delay(Duration::from_millis(1500));

    let session = start(left, right, CheckMode::Content);
    let mut last = std::time::Instant::now();
    let mut widest = Duration::ZERO;
    loop {
        let batch = session
            .batches()
            .recv_timeout(Duration::from_secs(20))
            .expect("session should emit a batch");
        widest = widest.max(last.elapsed());
        last = std::time::Instant::now();
        if batch.is_final {
            break;
        }
    }

    assert!(
        widest < Duration::from_millis(700),
        "no batch arrived for {} ms while a content read was running",
        widest.as_millis()
    );
}

#[test]
fn the_heartbeat_names_the_slow_file_and_ends_with_the_run() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("slow.bin", b"aaaa".to_vec());
    right.add_file("slow.bin", b"aaaa".to_vec());
    left.set_open_delay(Duration::from_millis(1300));

    let session = start(left, right, CheckMode::Content);
    let batches = collect(&session);

    assert!(
        batches
            .iter()
            .filter_map(|batch| batch.reading.as_ref())
            .any(|reading| {
                reading.rel.as_path().to_text() == "slow.bin" && reading.elapsed_ms >= 1000
            }),
        "a read running for over 1 s must be named with its age"
    );
    let last = batches.last().expect("collect returns the final batch");
    assert_eq!(
        last.reading, None,
        "the final batch has no read left running"
    );
    // Workers stay parked for later focus requests; the walker and the heartbeat must end.
    let parked = session.queue.workers();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while session
        .handles
        .iter()
        .filter(|handle| handle.is_finished())
        .count()
        < session.handles.len() - parked
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the heartbeat thread must exit after the final batch"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn cancel_during_a_slow_read_ends_the_heartbeat() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    left.add_file("slow.bin", b"aaaa".to_vec());
    right.add_file("slow.bin", b"aaaa".to_vec());
    left.set_open_delay(Duration::from_millis(3000));

    let session = start(left, right, CheckMode::Content);
    loop {
        let batch = session
            .batches()
            .recv_timeout(Duration::from_secs(5))
            .expect("a heartbeat arrives while the read runs");
        assert!(
            !batch.is_final,
            "the read must still run when the test cancels"
        );
        if batch.reading.is_some() {
            break;
        }
    }
    session.cancel();

    // Workers stay parked for later focus requests; the walker and the heartbeat must end.
    let parked = session.queue.workers();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while session
        .handles
        .iter()
        .filter(|handle| handle.is_finished())
        .count()
        < session.handles.len() - parked
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the heartbeat thread must exit within 1 s of cancel"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
