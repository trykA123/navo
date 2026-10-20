use std::time::Instant;

use rusty_core::OpKind;

use super::{Act, Stop, drawer_stops, line_stops, settle, shown_line_stops, step, step_item};
use crate::activity::{Activity, ActivityId, ActivityItem};
use crate::job_card::{JobInfo, Pause};
use crate::job_row::JobRowState;

fn job(id: u64, state: JobRowState) -> JobInfo {
    JobInfo {
        id,
        kind: OpKind::Copy,
        items: 2,
        from: Some("commander".into()),
        target: Some("/archive".into()),
        current: None,
        bytes_done: 1,
        bytes_total: 4,
        files_done: 1,
        files_total: 4,
        state,
        speed: None,
        pause: Pause::Off,
    }
}

fn items(jobs: &[JobInfo]) -> Vec<ActivityItem> {
    let mut activity = Activity::default();
    activity.sync(jobs, Instant::now());
    activity
        .primary()
        .into_iter()
        .chain(activity.rest())
        .cloned()
        .collect()
}

const ONE: ActivityId = ActivityId::Job(1);
const TWO: ActivityId = ActivityId::Job(2);

#[test]
fn a_lone_job_offers_pause_then_cancel() {
    let stops = line_stops(&items(&[job(1, JobRowState::Running)]));
    assert_eq!(
        stops,
        [
            Stop::Button(ONE, Act::Pause),
            Stop::Button(ONE, Act::Cancel)
        ]
    );
}

#[test]
fn a_paused_job_offers_resume() {
    let paused = JobInfo {
        pause: Pause::On,
        ..job(1, JobRowState::Running)
    };
    assert_eq!(
        line_stops(&items(&[paused]))[0],
        Stop::Button(ONE, Act::Resume)
    );
}

#[test]
fn other_items_wait_as_chips_before_the_all_button() {
    let failed = job(2, JobRowState::Failed("Access denied".into()));
    let stops = line_stops(&items(&[job(1, JobRowState::Running), failed]));
    assert_eq!(
        stops,
        [
            Stop::Button(TWO, Act::Show),
            Stop::Button(TWO, Act::Dismiss),
            Stop::Chip(ONE),
            Stop::All,
        ]
    );
}

#[test]
fn the_drawer_gives_a_row_stop_to_items_without_buttons() {
    let all = items(&[job(1, JobRowState::Running), job(2, JobRowState::Done)]);
    assert_eq!(
        drawer_stops(&all),
        [
            Stop::Button(ONE, Act::Pause),
            Stop::Button(ONE, Act::Cancel),
            Stop::Row(TWO),
        ]
    );
}

#[test]
fn left_and_right_wrap_around() {
    let stops = [Stop::Chip(ONE), Stop::Chip(TWO), Stop::All];
    assert_eq!(step(&stops, Some(Stop::All), true), Some(Stop::Chip(ONE)));
    assert_eq!(step(&stops, Some(Stop::Chip(ONE)), false), Some(Stop::All));
    assert_eq!(step(&stops, None, true), Some(Stop::Chip(ONE)));
}

#[test]
fn up_and_down_jump_between_items() {
    let stops = [
        Stop::Button(ONE, Act::Pause),
        Stop::Button(ONE, Act::Cancel),
        Stop::Row(TWO),
    ];
    let cancel = Some(Stop::Button(ONE, Act::Cancel));
    assert_eq!(step_item(&stops, cancel, true), Some(Stop::Row(TWO)));
    assert_eq!(
        step_item(&stops, Some(Stop::Row(TWO)), true),
        Some(Stop::Row(TWO))
    );
    assert_eq!(
        step_item(&stops, Some(Stop::Row(TWO)), false),
        Some(stops[0])
    );
}

#[test]
fn a_stop_that_went_away_settles_on_the_first() {
    let stops = [Stop::Chip(ONE), Stop::All];
    assert_eq!(settle(&stops, Some(Stop::Chip(TWO))), Some(Stop::Chip(ONE)));
    assert_eq!(settle(&stops, Some(Stop::All)), Some(Stop::All));
    assert_eq!(settle(&[], Some(Stop::All)), None);
}

#[test]
fn chips_that_do_not_fit_are_not_stops() {
    let all = items(&[
        job(1, JobRowState::Running),
        job(2, JobRowState::Running),
        job(3, JobRowState::Running),
    ]);
    let ids: Vec<ActivityId> = all.iter().map(|item| item.id).collect();
    let stops = shown_line_stops(&all, Some(1));
    assert!(stops.contains(&Stop::Chip(ids[1])));
    assert!(!stops.contains(&Stop::Chip(ids[2])));
    assert_eq!(stops.last(), Some(&Stop::All));
    assert_eq!(shown_line_stops(&all, None), line_stops(&all));
    assert!(!shown_line_stops(&all, Some(0)).contains(&Stop::Chip(ids[1])));
}
