use std::time::{Duration, Instant};

use super::*;

const READS: usize = 20;
const BUDGET: Duration = Duration::from_millis(5);

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release builds only")]
fn reading_three_panes_of_100_rows_takes_under_5_ms() {
    let dir = tmp();
    let path = session_file(&dir);
    put(&path, &three_panes_of_100_rows());
    let mut times: Vec<Duration> = (0..READS)
        .map(|_| {
            let start = Instant::now();
            let (session, _, _) = load_session(&path);
            let elapsed = start.elapsed();
            assert!(session.is_some());
            elapsed
        })
        .collect();
    times.sort_unstable();
    let median = times[READS / 2];
    assert!(median < BUDGET, "median {median:?} over {times:?}");
}
