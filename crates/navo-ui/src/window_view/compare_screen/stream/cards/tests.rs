//! Tests for a heavy card's detail text: the large card's status line and its line ranges.

use super::*;
use crate::compare_diff::{LargeProgress, LargeStage};

/// A large card with the given scan, found count and progress.
fn large(scan: CardScan, found: u32, progress: Option<LargeProgress>) -> LargeCard {
    LargeCard {
        hunks: Vec::new(),
        found,
        progress,
        scan,
    }
}

/// Progress through `done` of `total` units of the scan stage.
fn made(done: u64, total: u64) -> LargeProgress {
    LargeProgress {
        stage: LargeStage::Scan,
        done,
        total,
    }
}

#[test]
fn large_status_reads_running_done_and_full() {
    let running = large(CardScan::Running, 3, Some(made(25, 100)));
    assert_eq!(
        large_status(&running),
        ("3 changes so far".into(), Some("Comparing… 25 %".into()))
    );
    // One hunk reads as one change, and no total yet means no percent to read.
    let one = large(CardScan::Running, 1, Some(made(0, 0)));
    assert_eq!(
        large_status(&one),
        ("1 change so far".into(), Some("Comparing…".into()))
    );
    // Finished: the count stands on its own, with the equal pair saying nothing.
    let done = large(CardScan::Done, 4, Some(made(100, 100)));
    assert_eq!(large_status(&done), ("4 changes".into(), None));
    assert_eq!(
        large_status(&large(CardScan::Done, 0, None)),
        ("No changes".into(), None)
    );
    // Full: the card shows what it can and says the rest is one keystroke away (D26.4).
    let full = large(CardScan::Full, 6, Some(made(40, 100)));
    assert_eq!(
        large_status(&full),
        ("6+ changes · Enter opens the full diff".into(), None)
    );
}

#[test]
fn rows_text_is_one_based() {
    assert_eq!(rows_text(&(0..1)), "1");
    assert_eq!(rows_text(&(0..3)), "1–3");
    assert_eq!(rows_text(&(9..10)), "10");
    assert_eq!(rows_text(&(4..12)), "5–12");
    // A hunk the scan has not sized yet shows the line it started at.
    assert_eq!(rows_text(&(7..7)), "8");
}
