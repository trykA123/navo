use std::time::{Duration, Instant};

use super::*;

const ENTRIES: usize = 100_000;
const FIRST_BATCH_BUDGET: Duration = Duration::from_millis(20);

#[test]
#[ignore = "perf: run by hand with --run-ignored only"]
fn first_batch_of_100k_entries_arrives_within_20_ms() {
    let (dir, folder) = temp();
    for i in 0..ENTRIES {
        fs::File::create(dir.path().join(format!("f{i:06}"))).unwrap();
    }
    let mut first = None;
    let start = Instant::now();
    let mut sink = |_: EntryBatch| {
        first = Some(start.elapsed());
        ControlFlow::Break(())
    };
    LocalFs::new()
        .list(&folder, &mut sink, &CancelToken::new())
        .unwrap();
    let first = first.unwrap();
    assert!(first < FIRST_BATCH_BUDGET, "first batch took {first:?}");
}
