use std::mem::size_of;
use std::time::Duration;

use super::super::run::JOB_QUEUE;
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
    left.set_open_delay(Duration::from_millis(50));
    right.set_open_delay(Duration::from_millis(50));
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

    let session_bytes = session.shared.retained_bytes();
    let queue_bytes = JOB_QUEUE * (size_of::<u64>() + 2 * size_of::<usize>());
    let retained = session_bytes + queue_bytes;
    assert!(retained <= ROW_MEMORY_GOAL, "{retained} bytes retained");
}
