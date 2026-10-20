use std::time::{Duration, Instant};

use gpui_kit::base::TestSupportExt;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use rusty_core::OpKind;

use super::{JobCard, JobInfo, JobsSummary, Pause, PillFit, SpeedMeter, time_text};
use crate::job_row::JobRowState;
use crate::prelude::*;
use crate::test_window::open;

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
        pause: Pause::Off,
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
    let renamed = JobInfo {
        kind: OpKind::Rename,
        ..copy()
    };
    assert_eq!(renamed.name(), "Rename 4 items");
    assert_eq!(renamed.heading(), "Renaming 4 items");
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

#[test]
fn several_jobs_without_bytes_combine_their_files() {
    let no_bytes = |id, files_done| JobInfo {
        id,
        kind: OpKind::Delete,
        bytes_done: 0,
        bytes_total: 0,
        files_done,
        files_total: 12,
        ..copy()
    };
    let summary = JobsSummary::of(&[no_bytes(1, 3), no_bytes(2, 0)]);
    assert_eq!(summary.percent_text(), "12%");
}

#[test]
fn a_stall_decays_the_speed_and_then_hides_it() {
    let start = Instant::now();
    let at = |half_seconds: u32| start + Duration::from_millis(500) * half_seconds;
    let mut meter = SpeedMeter::default();
    meter.observe(1, 0, at(0));
    meter.observe(1, 10 * MB, at(1));
    let mut last = meter.speed(1).unwrap_or_default();
    for tick in 2..6 {
        meter.observe(1, 10 * MB, at(tick));
        let speed = meter.speed(1).unwrap_or_default();
        assert!(speed < last, "{speed} after {last}");
        last = speed;
    }
    for tick in 6..20 {
        meter.observe(1, 10 * MB, at(tick));
    }
    assert_eq!(meter.speed(1), None);
}

fn paused(pause: Pause) -> JobInfo {
    JobInfo { pause, ..copy() }
}

#[test]
fn a_paused_job_says_paused_instead_of_speed_and_time_left() {
    let job = paused(Pause::On);
    assert_eq!(job.status_text(), "25% · 1 of 12 files · Paused");
    assert_eq!(job.left(), None);
}

#[test]
fn a_job_asked_to_pause_says_pausing() {
    assert_eq!(
        paused(Pause::Asked).status_text(),
        "25% · 1 of 12 files · Pausing…"
    );
}

#[test]
fn a_paused_job_adds_no_speed_or_time_left_to_the_summary() {
    let summary = JobsSummary::of(&[
        copy(),
        JobInfo {
            pause: Pause::On,
            ..second()
        },
    ]);
    assert_eq!(summary.speed, Some(10 * MB));
    assert_eq!(summary.left, Some(Duration::from_secs(30)));
    assert!(summary.is_running);
    assert!(!summary.is_paused);
}

#[test]
fn a_summary_is_paused_when_every_running_job_is_paused() {
    let summary = JobsSummary::of(&[paused(Pause::On), with_state(JobRowState::Done)]);
    assert!(summary.is_paused);
    assert!(!summary.is_ended());
    assert!(!JobsSummary::of(&[paused(Pause::Asked)]).is_paused);
}

#[test]
fn holding_keeps_the_speed_and_skips_the_paused_time() {
    let start = Instant::now();
    let at = |seconds: u64| start + Duration::from_secs(seconds);
    let mut meter = SpeedMeter::default();
    meter.observe(1, 0, at(0));
    meter.observe(1, 10 * MB, at(1));
    for second in 2..30 {
        meter.hold(1, 10 * MB, at(second));
    }
    assert_eq!(meter.speed(1), Some(10 * MB));
    meter.observe(1, 20 * MB, at(30));
    assert_eq!(meter.speed(1), Some(10 * MB));
}

struct Under {
    clicks: usize,
}

impl Render for Under {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let done = JobInfo {
            state: JobRowState::Done,
            ..copy()
        };
        let card = JobCard::new(vec![done]).fade_out(Duration::from_millis(1));
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id("under")
                    .test_support()
                    .absolute()
                    .inset_0()
                    .on_click(cx.listener(|this, _, _, _| this.clicks += 1)),
            )
            .child(div().absolute().left_0().top_0().child(card))
    }
}

#[gpui_kit::test]
fn a_faded_card_neither_shows_nor_takes_clicks(cx: &mut TestAppContext) {
    let (window, under) = open(cx, |_, _| Under { clicks: 0 });
    let deadline = Instant::now() + Duration::from_secs(2);
    let card = loop {
        let card = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.find("job-card")
            })
            .expect("window");
        if !card.visible() || Instant::now() > deadline {
            break card;
        }
    };
    assert!(!card.visible());
    let middle = card.bounds().center();
    cx.update_window(window, |_, window, cx| {
        window.click_at("under", middle, cx);
    })
    .expect("window");
    assert_eq!(under.read_with(cx, |under, _| under.clicks), 1);
}
