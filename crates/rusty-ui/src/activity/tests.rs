use std::time::{Duration, Instant};

use rusty_core::OpKind;

use super::{Activity, ActivityId, ActivityItem, ActivityKind, DONE_FOR};
use crate::job_card::{JobInfo, Pause};
use crate::job_row::JobRowState;

const MB: u64 = 1024 * 1024;

fn copy(id: u64) -> JobInfo {
    JobInfo {
        id,
        kind: OpKind::Copy,
        items: 3,
        from: Some("commander".into()),
        target: Some(r"D:\Backup".into()),
        current: None,
        bytes_done: 42 * MB,
        bytes_total: 128 * MB,
        files_done: 42,
        files_total: 128,
        state: JobRowState::Running,
        speed: Some(84 * MB),
        pause: Pause::Off,
    }
}

fn with_state(job: JobInfo, state: JobRowState) -> JobInfo {
    JobInfo { state, ..job }
}

fn done(id: u64) -> JobInfo {
    with_state(copy(id), JobRowState::Done)
}

fn failed(id: u64) -> JobInfo {
    with_state(copy(id), JobRowState::Failed("Access denied".into()))
}

fn kinds(activity: &Activity) -> Vec<ActivityKind> {
    activity
        .primary()
        .into_iter()
        .chain(activity.rest())
        .map(|item| item.kind)
        .collect()
}

fn ids(activity: &Activity) -> Vec<ActivityId> {
    activity
        .primary()
        .into_iter()
        .chain(activity.rest())
        .map(|item| item.id)
        .collect()
}

fn primary(activity: &Activity) -> &ActivityItem {
    activity.primary().expect("the activity has an item")
}

#[test]
fn items_rank_failure_drag_job_done_background() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.set_background(7, "Indexing D:".into(), None);
    activity.sync(&[done(1)], now);
    activity.sync(&[done(1), copy(2), failed(3)], now);
    activity.set_drag(Some(2));
    assert_eq!(
        kinds(&activity),
        [
            ActivityKind::Failure,
            ActivityKind::Drag,
            ActivityKind::Job,
            ActivityKind::Done,
            ActivityKind::Background,
        ]
    );
    assert_eq!(activity.len(), 5);
}

#[test]
fn the_newest_item_of_a_kind_comes_first() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.sync(&[copy(1)], now);
    activity.sync(&[copy(1), copy(2)], now);
    assert_eq!(ids(&activity), [ActivityId::Job(2), ActivityId::Job(1)]);
}

#[test]
fn a_finished_job_turns_into_done() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.sync(&[copy(1)], now);
    assert_eq!(primary(&activity).kind, ActivityKind::Job);
    activity.sync(&[done(1)], now);
    assert_eq!(kinds(&activity), [ActivityKind::Done]);
    assert_eq!(primary(&activity).id, ActivityId::Job(1));
}

#[test]
fn done_expires_after_eight_seconds() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.sync(&[done(1)], now);
    activity.sync(&[done(1)], now + DONE_FOR - Duration::from_millis(1));
    assert_eq!(activity.len(), 1);
    assert_eq!(activity.next_expiry(), Some(now + DONE_FOR));
    activity.tick(now + DONE_FOR);
    assert_eq!(activity.len(), 0);
    activity.sync(&[done(1)], now + DONE_FOR);
    assert_eq!(activity.len(), 0, "an expired job does not come back");
}

#[test]
fn done_does_not_expire_while_held() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.sync(&[done(1)], now);
    activity.hold(ActivityId::Job(1), true, now + Duration::from_secs(5));
    assert_eq!(activity.next_expiry(), None);
    activity.tick(now + Duration::from_secs(60));
    assert_eq!(activity.len(), 1);
    let released = now + Duration::from_secs(60);
    activity.hold(ActivityId::Job(1), false, released);
    activity.tick(released + Duration::from_secs(2));
    assert_eq!(activity.len(), 1, "the 3 s left resume after release");
    activity.tick(released + Duration::from_secs(3));
    assert_eq!(activity.len(), 0);
}

#[test]
fn a_job_with_errors_turns_into_failure() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.sync(&[copy(1)], now);
    activity.sync(&[failed(1)], now);
    assert_eq!(kinds(&activity), [ActivityKind::Failure]);
}

#[test]
fn failure_stays_until_dismissed() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.sync(&[failed(1)], now);
    activity.sync(&[failed(1)], now + Duration::from_secs(600));
    activity.sync(&[], now + Duration::from_secs(600));
    assert_eq!(kinds(&activity), [ActivityKind::Failure]);
    activity.sync(&[failed(1)], now + Duration::from_secs(600));
    assert!(activity.dismiss(ActivityId::Job(1)));
    assert_eq!(activity.len(), 0);
    activity.sync(&[failed(1)], now + Duration::from_secs(601));
    assert_eq!(activity.len(), 0, "a dismissed failure does not come back");
}

#[test]
fn a_cancelled_job_leaves_no_item() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.sync(&[copy(1)], now);
    activity.sync(&[with_state(copy(1), JobRowState::Cancelled)], now);
    assert_eq!(activity.len(), 0);
}

#[test]
fn a_pinned_item_leads_until_it_ends() {
    let now = Instant::now();
    let mut activity = Activity::default();
    activity.sync(&[copy(1), copy(2), failed(3)], now);
    assert_eq!(primary(&activity).id, ActivityId::Job(3));
    assert!(activity.pin(ActivityId::Job(1)));
    assert_eq!(primary(&activity).id, ActivityId::Job(1));
    assert_eq!(activity.rest().count(), 2);
    activity.sync(&[done(1), copy(2), failed(3)], now);
    assert_eq!(primary(&activity).id, ActivityId::Job(3));
    assert!(!activity.pin(ActivityId::Job(9)));
}

#[test]
fn drag_and_background_come_and_go() {
    let mut activity = Activity::default();
    activity.set_drag(Some(3));
    activity.set_background(7, "Indexing D:".into(), Some(0.5));
    assert_eq!(activity.len(), 2);
    let background = activity.rest().next().expect("background item");
    assert_eq!(background.progress, Some(0.5));
    activity.set_drag(None);
    activity.end_background(7);
    assert_eq!(activity.len(), 0);
}

#[test]
fn a_job_reads_what_where_and_how_far() {
    let mut activity = Activity::default();
    let job = JobInfo {
        bytes_done: 42 * MB,
        bytes_total: 42 * MB + 84 * MB * 74,
        ..copy(1)
    };
    activity.sync(&[job], Instant::now());
    let item = primary(&activity);
    assert_eq!(item.title.as_ref(), r"Copying 128 files to D:\Backup");
    assert_eq!(
        item.detail.as_deref(),
        Some("42 of 128 · 84.0 MB/s · 1:14 left")
    );
    assert!(item.can_pause && item.can_cancel && !item.paused);
    assert!(item.progress.is_some_and(|fraction| fraction > 0.));
}

#[test]
fn a_paused_job_says_so() {
    let mut activity = Activity::default();
    let job = JobInfo {
        pause: Pause::On,
        files_total: 1,
        files_done: 0,
        ..copy(1)
    };
    activity.sync(&[job], Instant::now());
    let item = primary(&activity);
    assert_eq!(item.title.as_ref(), r"Copying 1 file to D:\Backup");
    assert_eq!(item.detail.as_deref(), Some("0 of 1 · Paused"));
    assert!(item.paused);
}

#[test]
fn a_failure_counts_the_files_left_behind() {
    let mut activity = Activity::default();
    let job = JobInfo {
        files_done: 126,
        ..failed(1)
    };
    activity.sync(&[job], Instant::now());
    let item = primary(&activity);
    assert_eq!(item.title.as_ref(), "2 files could not be copied");
    assert_eq!(item.detail.as_deref(), Some("Access denied"));
    assert!(!item.can_pause && !item.can_cancel);
}

#[test]
fn done_names_the_target_folder() {
    let mut activity = Activity::default();
    let job = JobInfo {
        kind: OpKind::Move,
        target: Some(r"C:\Users\admin\Projects\".into()),
        ..done(1)
    };
    activity.sync(&[job], Instant::now());
    let item = primary(&activity);
    assert_eq!(item.title.as_ref(), "Moved 3 items to Projects");
    assert_eq!(item.detail, None);
    assert_eq!(item.progress, None);
}

#[test]
fn recycle_and_delete_read_without_a_target() {
    let mut activity = Activity::default();
    let recycled = JobInfo {
        kind: OpKind::Recycle,
        items: 1,
        target: None,
        ..done(1)
    };
    activity.sync(&[recycled], Instant::now());
    assert_eq!(
        primary(&activity).title.as_ref(),
        "Moved 1 item to the Recycle Bin"
    );
    let deleting = JobInfo {
        kind: OpKind::Delete,
        target: None,
        ..copy(2)
    };
    activity.sync(&[deleting], Instant::now());
    let titles: Vec<_> = activity.rest().map(|item| item.title.clone()).collect();
    assert_eq!(primary(&activity).title.as_ref(), "Deleting 128 files");
    assert_eq!(titles, ["Moved 1 item to the Recycle Bin"]);
}

#[test]
fn rename_reads_without_a_target() {
    let mut activity = Activity::default();
    let renaming = JobInfo {
        kind: OpKind::Rename,
        target: None,
        ..copy(1)
    };
    activity.sync(&[renaming], Instant::now());
    assert_eq!(primary(&activity).title.as_ref(), "Renaming 128 files");
    let renamed = JobInfo {
        kind: OpKind::Rename,
        items: 3,
        target: None,
        ..done(2)
    };
    activity.sync(&[renamed], Instant::now());
    assert_eq!(primary(&activity).title.as_ref(), "Renamed 3 items");
}

#[test]
fn a_drag_shows_its_keys() {
    let mut activity = Activity::default();
    activity.set_drag(Some(3));
    let item = primary(&activity);
    assert_eq!(item.title.as_ref(), "Dragging 3 items");
    assert_eq!(
        item.detail.as_deref(),
        Some("Ctrl copy · Shift move · Alt shortcut · Esc cancel")
    );
}

#[test]
fn a_chip_names_each_item_in_a_few_words() {
    let now = Instant::now();
    let mut activity = Activity::default();
    let moving = JobInfo {
        kind: OpKind::Move,
        bytes_done: 64 * MB,
        bytes_total: 100 * MB,
        ..copy(1)
    };
    activity.sync(&[moving, done(2), failed(3)], now);
    activity.set_drag(Some(2));
    let chips: Vec<_> = activity
        .primary()
        .into_iter()
        .chain(activity.rest())
        .map(|item| item.chip.to_string())
        .collect();
    assert_eq!(
        chips,
        [
            "Copy failed",
            "Dragging 2 items",
            "Move 64%",
            "Copied 3 items"
        ]
    );
}

#[test]
fn a_recycle_chip_says_recycle_not_delete() {
    let mut activity = Activity::default();
    let recycled = JobInfo {
        kind: OpKind::Recycle,
        items: 1,
        target: None,
        ..done(1)
    };
    activity.sync(&[recycled], Instant::now());
    let chip = activity.primary().map(|item| item.chip.to_string());
    assert_eq!(chip.as_deref(), Some("Recycled 1 item"));
}

#[test]
fn a_paused_job_says_so_on_its_chip() {
    let mut activity = Activity::default();
    let paused = JobInfo {
        pause: Pause::On,
        ..copy(1)
    };
    activity.sync(&[paused], Instant::now());
    let chip = activity.primary().map(|item| item.chip.to_string());
    assert_eq!(chip.as_deref(), Some("Copy 32% · Paused"));
}
