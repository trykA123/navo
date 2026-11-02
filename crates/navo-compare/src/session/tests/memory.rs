use std::time::Duration;

use super::*;

const PAIRED_ROWS: usize = 100_000;
const ROW_MEMORY_GOAL: usize = 50 * 1024 * 1024;

#[test]
fn a_hundred_thousand_checking_rows_fit_in_fifty_megabytes() {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for index in 0..PAIRED_ROWS {
        let name = format!("f{index:06}.bin");
        left.add_file(&name, vec![0xAA; 1]);
        right.add_file(&name, vec![0xAA; 1]);
    }
    left.set_open_delay(Duration::from_millis(500));
    right.set_open_delay(Duration::from_millis(500));
    let session = start(left, right, CheckMode::Content);

    let first = session
        .batches()
        .recv_timeout(Duration::from_secs(20))
        .expect("initial checking batch");
    assert_eq!(first.rows.len(), PAIRED_ROWS);
    assert!(
        first
            .rows
            .iter()
            .all(|row| row.raw_status == Status::Checking)
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while session.queue.remaining() < PAIRED_ROWS {
        assert!(std::time::Instant::now() < deadline, "queue did not fill");
        std::thread::yield_now();
    }

    let session_bytes = session.shared.retained_bytes();
    let queue_bytes = session.queue.retained_bytes();
    let retained = session_bytes + queue_bytes;
    assert!(retained <= ROW_MEMORY_GOAL, "{retained} bytes retained");
}
