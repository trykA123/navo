use navo_compare::model::{ContentClass, Generation, Row, Status};
use navo_compare::session::{RowBatch, StopBound, Totals};
use navo_ui::browser::{CompareUpdate, StopBound as UiBound};

use crate::compare::convert::update_of;
use crate::compare::handle::CompareEvent;

#[test]
fn a_walk_stopped_at_the_limit_becomes_a_stopped_update() {
    let stopped = |bound| CompareEvent::StoppedAtLimit {
        generation: Generation::new(3),
        entries: 200_000,
        bound,
    };
    assert_eq!(
        update_of(stopped(StopBound::Entries)),
        CompareUpdate::Stopped {
            generation: 3,
            entries: 200_000,
            bound: UiBound::Entries
        }
    );
    assert_eq!(
        update_of(stopped(StopBound::Time)),
        CompareUpdate::Stopped {
            generation: 3,
            entries: 200_000,
            bound: UiBound::Time
        }
    );
}

#[test]
fn a_continued_walk_becomes_a_continued_update() {
    let event = CompareEvent::Continued {
        generation: Generation::new(3),
    };
    assert_eq!(update_of(event), CompareUpdate::Continued { generation: 3 });
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
    let CompareUpdate::Rows { rows, .. } = update_of(CompareEvent::Rows(batch)) else {
        panic!("rows update");
    };
    assert_eq!(rows[0].differences_inside, 3);
}
