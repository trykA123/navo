//! Unit tests for `CompareRows`: phase, slow-path and reading telemetry.

use super::super::hooks::{ComparePhase, CompareUpdate, SlowPath};
use super::{Applied, CompareRows};

/// Opens `generation` with an empty batch, so it is the newest one.
fn start(rows: &mut CompareRows, generation: u64) {
    let update = CompareUpdate::Rows {
        generation,
        rows: Vec::new(),
        progress: Vec::new(),
    };
    assert_eq!(rows.apply(update), Applied::Changed);
}

#[test]
fn phase_progress_of_the_current_generation_changes_the_rows() {
    let mut rows = CompareRows::new();
    start(&mut rows, 2);
    let update = CompareUpdate::PhaseProgress {
        generation: 2,
        phase: ComparePhase::QuickCheck,
        done: 4,
        total: Some(10),
    };
    assert_eq!(rows.apply(update), Applied::Changed);
    assert_eq!(rows.phase(), Some(ComparePhase::QuickCheck));
}

#[test]
fn a_slow_path_of_the_current_generation_changes_the_rows() {
    let mut rows = CompareRows::new();
    start(&mut rows, 2);
    let update = CompareUpdate::SlowPath {
        generation: 2,
        path: "slow.bin".to_owned(),
        elapsed_ms: 250,
    };
    assert_eq!(rows.apply(update), Applied::Changed);
    assert_eq!(
        rows.slow_paths(),
        [SlowPath {
            path: "slow.bin".to_owned(),
            elapsed_ms: 250,
        }]
        .as_slice()
    );
}

#[test]
fn the_latest_phase_and_slow_paths_read_back_after_updates() {
    let mut rows = CompareRows::new();
    start(&mut rows, 2);
    rows.apply(CompareUpdate::PhaseProgress {
        generation: 2,
        phase: ComparePhase::Listing,
        done: 0,
        total: None,
    });
    rows.apply(CompareUpdate::PhaseProgress {
        generation: 2,
        phase: ComparePhase::ContentCheck,
        done: 3,
        total: Some(9),
    });
    rows.apply(CompareUpdate::SlowPath {
        generation: 2,
        path: "a.bin".to_owned(),
        elapsed_ms: 250,
    });
    rows.apply(CompareUpdate::SlowPath {
        generation: 2,
        path: "b.bin".to_owned(),
        elapsed_ms: 400,
    });
    assert_eq!(
        rows.phase(),
        Some(ComparePhase::ContentCheck),
        "the newest phase wins"
    );
    assert_eq!(
        rows.slow_paths(),
        [
            SlowPath {
                path: "a.bin".to_owned(),
                elapsed_ms: 250,
            },
            SlowPath {
                path: "b.bin".to_owned(),
                elapsed_ms: 400,
            },
        ]
        .as_slice(),
        "slow paths keep arrival order"
    );
    start(&mut rows, 3);
    assert_eq!(rows.phase(), None, "a new compare starts without a phase");
    assert!(
        rows.slow_paths().is_empty(),
        "a new compare starts without slow paths"
    );
}

#[test]
fn slow_paths_keep_only_the_newest_64() {
    let mut rows = CompareRows::new();
    start(&mut rows, 2);
    for step in 0..65 {
        let update = CompareUpdate::SlowPath {
            generation: 2,
            path: format!("{step}.bin"),
            elapsed_ms: 0,
        };
        assert_eq!(rows.apply(update), Applied::Changed);
    }
    assert_eq!(rows.slow_paths().len(), 64, "the list is bounded");
    assert_eq!(rows.slow_paths()[0].path, "1.bin", "the oldest path drops");
    assert_eq!(
        rows.slow_paths()[63].path,
        "64.bin",
        "the newest path is last"
    );
}

#[test]
fn telemetry_of_an_older_generation_is_ignored() {
    let mut rows = CompareRows::new();
    start(&mut rows, 2);
    let phase = CompareUpdate::PhaseProgress {
        generation: 1,
        phase: ComparePhase::Listing,
        done: 0,
        total: None,
    };
    let slow = CompareUpdate::SlowPath {
        generation: 1,
        path: "old.bin".to_owned(),
        elapsed_ms: 300,
    };
    assert_eq!(rows.apply(phase), Applied::Ignored);
    assert_eq!(rows.apply(slow), Applied::Ignored);
}

#[test]
fn the_running_read_lasts_until_the_next_batch_or_the_end() {
    let reading = |generation, elapsed_ms| CompareUpdate::Reading {
        generation,
        path: "a/b.bin".to_owned(),
        elapsed_ms,
    };
    let mut rows = CompareRows::new();
    start(&mut rows, 2);
    assert_eq!(rows.apply(reading(1, 900)), Applied::Ignored);
    assert_eq!(rows.reading(), None, "an older generation is ignored");

    assert_eq!(rows.apply(reading(2, 6200)), Applied::Changed);
    let expected = SlowPath {
        path: "a/b.bin".to_owned(),
        elapsed_ms: 6200,
    };
    assert_eq!(rows.reading(), Some(&expected));

    start(&mut rows, 2);
    assert_eq!(rows.reading(), None, "a batch without a read clears it");

    assert_eq!(rows.apply(reading(2, 300)), Applied::Changed);
    assert_eq!(
        rows.apply(CompareUpdate::Finished { generation: 2 }),
        Applied::Changed
    );
    assert_eq!(rows.reading(), None, "a finished compare reads nothing");
}
