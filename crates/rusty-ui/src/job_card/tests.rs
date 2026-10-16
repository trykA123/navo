use std::time::{Duration, Instant};

use gpui_kit::px;
use rusty_core::OpKind;

use super::{JobInfo, JobsSummary, PillFit, SpeedMeter, time_text};
use crate::job_row::JobRowState;

const MB: u64 = 1024 * 1024;

fn copy() -> JobInfo {
    JobInfo {
        id: 1,
        kind: OpKind::Copy,
        items: 4,
        from: Some("commander".into()),
        target: Some(r"Z:\archive\2026-10".into()),
        current: Some("Sinaia-2026-10.7z".into()),
        bytes_done: 100 * MB,
        bytes_total: 400 * MB,
        files_done: 1,
        files_total: 12,
        state: JobRowState::Running,
        speed: Some(10 * MB),
    }
}

fn with_state(state: JobRowState) -> JobInfo {
    JobInfo { state, ..copy() }
}

#[test]
fn a_job_names_its_verb_and_items() {
    assert_eq!(copy().name(), "Copy 4 items");
    let moved = JobInfo {
        kind: OpKind::Move,
        items: 1,
        ..copy()
    };
    assert_eq!(moved.name(), "Move 1 item");
    assert_eq!(moved.heading(), "Moving 1 item");
    assert_eq!(copy().heading(), "Copying 4 items");
    assert_eq!(with_state(JobRowState::Done).heading(), "Copied 4 items");
    assert_eq!(with_state(JobRowState::Cancelled).heading(), "Copy 4 items");
}

#[test]
fn the_route_reads_from_to() {
    assert_eq!(
        copy().route().as_deref(),
        Some(r"commander → Z:\archive\2026-10")
    );
    let recycle = JobInfo {
        kind: OpKind::Recycle,
        target: None,
        ..copy()
    };
    assert_eq!(recycle.route().as_deref(), Some("commander → Recycle Bin"));
    let delete = JobInfo {
        kind: OpKind::Delete,
        target: None,
        ..copy()
    };
    assert_eq!(delete.route().as_deref(), Some("in commander"));
}

#[test]
fn progress_reads_bytes_files_speed_and_time_left() {
    let job = copy();
    assert_eq!(job.percent_text(), "25%");
    assert_eq!(job.bytes_text(), "100.0 MB / 400.0 MB");
    assert_eq!(job.files_text(), "1 of 12 files");
    assert_eq!(job.left(), Some(Duration::from_secs(30)));
    assert_eq!(
        job.status_text(),
        "25% · 1 of 12 files · 10.0 MB/s · 0:30 left"
    );
}

#[test]
fn without_a_speed_there_is_no_time_left() {
    let job = JobInfo {
        speed: None,
        ..copy()
    };
    assert_eq!(job.left(), None);
    assert_eq!(job.status_text(), "25% · 1 of 12 files");
}

#[test]
fn a_job_without_bytes_counts_files() {
    let job = JobInfo {
        bytes_done: 0,
        bytes_total: 0,
        files_done: 3,
        files_total: 12,
        ..copy()
    };
    assert_eq!(job.percent_text(), "25%");
}

#[test]
fn ended_jobs_name_their_state() {
    assert_eq!(with_state(JobRowState::Done).status_text(), "Done");
    assert_eq!(
        with_state(JobRowState::Cancelled).status_text(),
        "Cancelled"
    );
    assert_eq!(
        with_state(JobRowState::Failed("denied".into())).status_text(),
        "Failed"
    );
}

#[test]
fn times_read_minutes_and_seconds_then_hours() {
    assert_eq!(time_text(Duration::from_secs(14)), "0:14");
    assert_eq!(time_text(Duration::from_secs(725)), "12:05");
    assert_eq!(time_text(Duration::from_secs(3723)), "1:02:03");
}

fn second() -> JobInfo {
    JobInfo {
        id: 2,
        kind: OpKind::Move,
        items: 1,
        bytes_done: 0,
        bytes_total: 400 * MB,
        speed: Some(5 * MB),
        ..copy()
    }
}

#[test]
fn several_jobs_combine_their_bytes_and_keep_the_longest_time_left() {
    let summary = JobsSummary::of(&[copy(), second()]);
    assert_eq!(summary.count, 2);
    assert_eq!(summary.percent_text(), "12%");
    assert_eq!(summary.left, Some(Duration::from_secs(80)));
    assert_eq!(summary.speed, Some(15 * MB));
    assert_eq!(summary.name, "2 jobs");
    assert_eq!(JobsSummary::of(&[copy()]).name, "Copy 4 items");
}

#[test]
fn a_summary_knows_when_every_job_ended() {
    let done = with_state(JobRowState::Done);
    assert!(JobsSummary::of(std::slice::from_ref(&done)).is_ended());
    assert!(!JobsSummary::of(&[done.clone(), copy()]).is_ended());
    let failed = with_state(JobRowState::Failed("denied".into()));
    let summary = JobsSummary::of(&[done, failed]);
    assert!(!summary.is_ended());
    assert!(summary.has_failed);
    assert_eq!(summary.word(), Some("Failed"));
    assert_eq!(JobsSummary::of(&[copy()]).word(), None);
}

#[test]
fn the_pill_drops_the_name_then_the_time_left_as_the_window_narrows() {
    assert_eq!(PillFit::for_width(px(1280.)), PillFit::Full);
    assert_eq!(PillFit::for_width(px(1100.)), PillFit::Full);
    assert_eq!(PillFit::for_width(px(1099.)), PillFit::NoName);
    assert_eq!(PillFit::for_width(px(960.)), PillFit::NoName);
    assert_eq!(PillFit::for_width(px(959.)), PillFit::Bare);
    assert!(PillFit::Full.slot_width() > PillFit::NoName.slot_width());
    assert!(PillFit::NoName.slot_width() > PillFit::Bare.slot_width());
}

#[test]
fn the_meter_smooths_bytes_per_second() {
    let start = Instant::now();
    let mut meter = SpeedMeter::default();
    meter.observe(1, 0, start);
    assert_eq!(meter.speed(1), None);
    meter.observe(1, 10 * MB, start + Duration::from_secs(1));
    assert_eq!(meter.speed(1), Some(10 * MB));
    meter.observe(1, 30 * MB, start + Duration::from_secs(2));
    let speed = meter.speed(1).unwrap_or_default();
    assert!(speed > 10 * MB && speed < 20 * MB, "{speed}");
}

#[test]
fn the_meter_ignores_samples_that_come_too_soon() {
    let start = Instant::now();
    let mut meter = SpeedMeter::default();
    meter.observe(1, 0, start);
    meter.observe(1, 10 * MB, start + Duration::from_millis(10));
    assert_eq!(meter.speed(1), None);
}

#[test]
fn the_meter_forgets_jobs_that_are_gone() {
    let start = Instant::now();
    let mut meter = SpeedMeter::default();
    meter.observe(1, 0, start);
    meter.observe(1, MB, start + Duration::from_secs(1));
    meter.retain(&[2]);
    assert_eq!(meter.speed(1), None);
}
