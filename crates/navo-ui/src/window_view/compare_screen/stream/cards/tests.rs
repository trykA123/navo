//! Tests for a heavy card's detail: the large card's status line and ranges, and the binary
//! card's tinted slots and status words.

use super::hex;
use super::image;
use super::*;
use crate::compare_diff::{
    ByteProgress, BytesCard, HexWindow, ImageCard, LargeProgress, LargeStage,
};

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

/// A hex window of `bytes` on each side, with the left's changed range `changed` (D26.8).
fn window(bytes: usize, changed: Range<u64>) -> HexWindow {
    HexWindow {
        left_start: 0x20,
        right_start: 0x20,
        left: vec![b'a'; bytes],
        right: vec![b'b'; bytes],
        changed_left: changed.clone(),
        changed_right: changed,
        wide: (false, false),
    }
}

/// A binary card with the given scan, window and byte-scan progress.
fn bytes(scan: CardScan, window: Option<HexWindow>, progress: Option<ByteProgress>) -> BytesCard {
    BytesCard {
        window,
        progress,
        scan,
    }
}

#[test]
fn window_tints_mark_only_the_changed_bytes() {
    // A range inside the row marks its own slots and nothing else.
    assert_eq!(
        hex::window_tints(0x20, &(0x23..0x25)),
        [(3, true), (4, true)]
    );
    // A range that runs past the row is cut at its 16 bytes.
    assert_eq!(
        hex::window_tints(0x20, &(0x2e..0x40)),
        [(14, true), (15, true)]
    );
    // Ranges before, after or empty mark no slot of this row.
    assert!(hex::window_tints(0x20, &(0x10..0x20)).is_empty());
    assert!(hex::window_tints(0x20, &(0x30..0x32)).is_empty());
    assert!(hex::window_tints(0x20, &(0x24..0x24)).is_empty());
}

#[test]
fn hex_status_reads_first_change_and_no_changes() {
    // The card stops at the first region it found (D26.4, D26.8).
    let full = bytes(CardScan::Full, Some(window(16, 0x2a..0x2c)), None);
    assert_eq!(hex::hex_status(&full), "First change at byte 0x2A");
    assert_eq!(
        hex::hex_status(&bytes(CardScan::Done, None, None)),
        "No changes"
    );
    // While the byte scan runs it says how far it got, or nothing at all yet.
    let going = bytes(
        CardScan::Running,
        None,
        Some(ByteProgress {
            done: 40,
            total: 100,
            coarse: false,
        }),
    );
    assert_eq!(hex::hex_status(&going), "Comparing… 40 %");
    assert_eq!(
        hex::hex_status(&bytes(
            CardScan::Running,
            None,
            Some(ByteProgress {
                done: 0,
                total: 0,
                coarse: true,
            }),
        )),
        "Comparing…"
    );
}

/// An image card with the given score and note, without thumbnails.
fn image(similarity: Option<u32>, note: Option<&'static str>) -> ImageCard {
    ImageCard {
        similarity,
        note,
        left: None,
        right: None,
    }
}

#[test]
fn image_status_prefers_the_score_then_the_note() {
    // The score is the card's only number, so it wins over any note (D26.6).
    assert_eq!(
        image::image_status(&image(Some(80), Some("compare-image-note-unreadable"))),
        "80 % similar"
    );
    // Without a score the note says why; with neither the job has not answered yet.
    assert_eq!(
        image::image_status(&image(None, Some("compare-image-note-unreadable"))),
        "A side cannot be read as a picture."
    );
    assert_eq!(image::image_status(&image(None, None)), "Comparing…");
}
