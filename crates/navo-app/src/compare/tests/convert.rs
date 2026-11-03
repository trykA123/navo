use navo_compare::model::Generation;
use navo_compare::session::StopBound;
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
