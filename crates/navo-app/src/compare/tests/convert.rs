//! Engine events as UI updates, including the ordered D5 telemetry forwarding.

use navo_compare::model::{ContentClass, Generation, RelPath, Row, Status};
use navo_compare::session::{
    ComparePhase as EnginePhase, PhaseTelemetry, RowBatch, SlowEntry, StopBound, Totals,
};
use navo_core::ProviderPath;
use navo_ui::browser::{ComparePhase, CompareUpdate, StopBound as UiBound};

use crate::compare::convert::updates_of;
use crate::compare::handle::CompareEvent;

#[test]
fn a_walk_stopped_at_the_limit_becomes_a_stopped_update() {
    let stopped = |bound| CompareEvent::StoppedAtLimit {
        generation: Generation::new(3),
        entries: 200_000,
        bound,
    };
    assert_eq!(
        updates_of(stopped(StopBound::Entries)),
        [CompareUpdate::Stopped {
            generation: 3,
            entries: 200_000,
            bound: UiBound::Entries
        }]
    );
    assert_eq!(
        updates_of(stopped(StopBound::Time)),
        [CompareUpdate::Stopped {
            generation: 3,
            entries: 200_000,
            bound: UiBound::Time
        }]
    );
}

#[test]
fn a_continued_walk_becomes_a_continued_update() {
    let event = CompareEvent::Continued {
        generation: Generation::new(3),
    };
    assert_eq!(
        updates_of(event),
        [CompareUpdate::Continued { generation: 3 }]
    );
}

#[test]
fn a_batch_with_phase_and_two_slow_entries_keeps_the_forwarding_order() {
    let slow = |path: &str, elapsed_ms| SlowEntry {
        rel: RelPath::new(ProviderPath::from_text(path)),
        elapsed_ms,
    };
    let batch = RowBatch {
        generation: Generation::new(4),
        phase: Some(PhaseTelemetry {
            phase: EnginePhase::QuickCheck,
            done: 3,
            total: Some(9),
        }),
        slow: vec![slow("a.bin", 250), slow("b.bin", 400)],
        ..RowBatch::default()
    };
    assert_eq!(
        updates_of(CompareEvent::Rows(batch)),
        [
            CompareUpdate::Rows {
                generation: 4,
                rows: Vec::new(),
                progress: Vec::new(),
            },
            CompareUpdate::PhaseProgress {
                generation: 4,
                phase: ComparePhase::QuickCheck,
                done: 3,
                total: Some(9),
            },
            CompareUpdate::SlowPath {
                generation: 4,
                path: "a.bin".to_owned(),
                elapsed_ms: 250,
            },
            CompareUpdate::SlowPath {
                generation: 4,
                path: "b.bin".to_owned(),
                elapsed_ms: 400,
            },
        ]
    );
}

#[test]
fn a_folder_row_keeps_its_count_of_differences_inside() {
    let row = Row {
        id: 7,
        left: None,
        right: None,
        raw_status: Status::Same,
        display_status: Status::Same,
        generation: Generation::new(1),
        content: ContentClass::Unknown,
        contains_differences: true,
        differences_inside: 3,
    };
    let batch = RowBatch {
        generation: Generation::new(1),
        rows: vec![row],
        progress: Vec::new(),
        totals: Totals::default(),
        is_final: false,
        problem: None,
        limit: None,
        ..RowBatch::default()
    };
    let mut updates = updates_of(CompareEvent::Rows(batch));
    let CompareUpdate::Rows { rows, .. } = updates.remove(0) else {
        panic!("rows update");
    };
    assert_eq!(rows[0].differences_inside, 3);
}
