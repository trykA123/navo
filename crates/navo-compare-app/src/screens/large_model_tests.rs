//! Tests for the large compare model: meter fractions, generation handling, tiles, notes and labels.

use sidewise_ui::browser::{ComparePhase, CompareTotals, CompareUpdate, SlowPath};

use super::large_model::{Meter, PhaseMeters, phase_label, reading_line, slow_note, tiles};

/// Builds one phase progress update for `generation`.
fn progress(generation: u64, phase: ComparePhase, done: u64, total: Option<u64>) -> CompareUpdate {
    CompareUpdate::PhaseProgress {
        generation,
        phase,
        done,
        total,
    }
}

/// Every fraction rule: unknown total, empty total, part done and a done value past the total.
#[test]
fn fraction_rules() {
    assert_eq!(
        Meter {
            done: 3,
            total: None
        }
        .fraction(),
        None
    );
    assert_eq!(
        Meter {
            done: 3,
            total: Some(0)
        }
        .fraction(),
        Some(1.0)
    );
    assert_eq!(
        Meter {
            done: 1,
            total: Some(4)
        }
        .fraction(),
        Some(0.25)
    );
    assert_eq!(
        Meter {
            done: 9,
            total: Some(5)
        }
        .fraction(),
        Some(1.0)
    );
}

/// Each stage's progress lands in its own slot with the reported done and total.
#[test]
fn observe_records_each_phase() {
    let mut meters = PhaseMeters::default();
    assert!(meters.observe(&progress(1, ComparePhase::Listing, 4, None)));
    assert!(meters.observe(&progress(1, ComparePhase::QuickCheck, 2, Some(9))));
    assert!(meters.observe(&progress(1, ComparePhase::ContentCheck, 1, Some(3))));

    assert_eq!(
        meters.meter(ComparePhase::Listing),
        Meter {
            done: 4,
            total: None
        }
    );
    assert_eq!(
        meters.meter(ComparePhase::QuickCheck),
        Meter {
            done: 2,
            total: Some(9)
        }
    );
    assert_eq!(
        meters.meter(ComparePhase::ContentCheck),
        Meter {
            done: 1,
            total: Some(3)
        }
    );
}

/// An update from an older generation changes no meter and reports nothing.
#[test]
fn observe_ignores_older_generation() {
    let mut meters = PhaseMeters::default();
    assert!(meters.observe(&progress(5, ComparePhase::Listing, 4, Some(8))));
    assert!(!meters.observe(&progress(4, ComparePhase::Listing, 1, Some(8))));
    assert_eq!(
        meters.meter(ComparePhase::Listing),
        Meter {
            done: 4,
            total: Some(8)
        }
    );
}

/// A newer generation resets the strip and reports the reset even for an empty row batch.
#[test]
fn newer_generation_resets_meters() {
    let mut meters = PhaseMeters::default();
    assert!(meters.observe(&progress(1, ComparePhase::Listing, 7, Some(8))));
    assert!(meters.observe(&CompareUpdate::Rows {
        generation: 2,
        rows: vec![],
        progress: vec![],
    }));
    assert_eq!(meters.meter(ComparePhase::Listing), Meter::default());
}

/// A finished update of the stored generation is not a change.
#[test]
fn observe_ignores_finished_of_same_generation() {
    let mut meters = PhaseMeters::default();
    assert!(meters.observe(&progress(1, ComparePhase::Listing, 2, None)));
    assert!(!meters.observe(&CompareUpdate::Finished { generation: 1 }));
    assert_eq!(
        meters.meter(ComparePhase::Listing),
        Meter {
            done: 2,
            total: None
        }
    );
}

/// The four tiles sum their totals fields and keep the D5 order and text.
#[test]
fn tiles_sum_totals() {
    let totals = CompareTotals {
        different: 2,
        only_left: 1,
        only_right: 3,
        conflicts: 1,
        same: 5,
        same_size_time: 2,
        checking: 4,
        ..CompareTotals::default()
    };
    let tiles = tiles(&totals);
    let values: [u64; 4] = tiles.map(|tile| tile.value);
    assert_eq!(values, [7, 3, 7, 4]);
    assert_eq!(tiles[0].label, "Differences");
    assert_eq!(tiles[0].note, "found so far");
    assert_eq!(tiles[3].label, "Checking");
    assert_eq!(tiles[3].note, "content check queue");
}

/// The slow note stays empty with no files and names the count and the slowest time otherwise.
#[test]
fn slow_note_texts() {
    assert_eq!(slow_note(&[]), None);
    assert_eq!(
        slow_note(&[SlowPath {
            path: "one.bin".to_owned(),
            elapsed_ms: 4200,
        }]),
        Some("1 slow file so far; it took 4.2 s.".to_owned())
    );
    assert_eq!(
        slow_note(&[
            SlowPath {
                path: "one.bin".to_owned(),
                elapsed_ms: 1000,
            },
            SlowPath {
                path: "two.bin".to_owned(),
                elapsed_ms: 2500,
            },
        ]),
        Some("2 slow files so far; the slowest took 2.5 s.".to_owned())
    );
}

/// Every stage has its heading and sub-line.
#[test]
fn phase_labels() {
    assert_eq!(
        phase_label(ComparePhase::Listing),
        ("Listing", "Both trees, breadth-first")
    );
    assert_eq!(
        phase_label(ComparePhase::QuickCheck),
        ("Size and time", "The quick check, no reads")
    );
    assert_eq!(
        phase_label(ComparePhase::ContentCheck),
        ("Content check", "Visible rows first")
    );
}

/// A repeated identical progress update changes nothing, so `observe` reports false.
#[test]
fn repeated_progress_is_not_a_change() {
    let mut meters = PhaseMeters::default();
    assert!(meters.observe(&progress(1, ComparePhase::QuickCheck, 2, Some(9))));
    assert!(!meters.observe(&progress(1, ComparePhase::QuickCheck, 2, Some(9))));
}

/// `Totals` and `SlowPath` updates of a newer generation reset the meters like progress does.
#[test]
fn totals_and_slow_path_reset_at_a_newer_generation() {
    let updates = [
        CompareUpdate::Totals {
            generation: 2,
            totals: CompareTotals::default(),
        },
        CompareUpdate::SlowPath {
            generation: 2,
            path: "a".into(),
            elapsed_ms: 9,
        },
    ];
    for update in updates {
        let mut meters = PhaseMeters::default();
        assert!(meters.observe(&progress(1, ComparePhase::Listing, 7, Some(8))));
        assert!(meters.observe(&update));
        assert_eq!(meters.meter(ComparePhase::Listing), Meter::default());
    }
}

/// The running read shows its path and age from 1 s on, and nothing before.
#[test]
fn reading_line_texts() {
    let read = |elapsed_ms| SlowPath {
        path: "a/b.bin".to_owned(),
        elapsed_ms,
    };
    assert_eq!(
        reading_line(Some(&read(6200))),
        Some("Reading a/b.bin · 6.2 s".to_owned())
    );
    assert_eq!(reading_line(Some(&read(400))), None);
    assert_eq!(reading_line(None), None);
}
