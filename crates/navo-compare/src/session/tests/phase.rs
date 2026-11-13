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
