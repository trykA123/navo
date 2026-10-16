use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext, px};
use rusty_core::{Location, OpKind};
use rusty_fs::{CancelToken, Progress};

use super::{fake, home_beside_dest, open_options_sized, open_with, options_for, press};
use crate::browser::{JobStatus, JobView};
use crate::window_view::FerriteWindow;

const MB: u64 = 1024 * 1024;

fn at(path: &str) -> Location {
    Location::new(path).unwrap()
}

fn open_at(cx: &mut TestAppContext, width: f32) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    open_options_sized(cx, options_for(fs, state), px(width))
}

struct Fake {
    id: u64,
    kind: OpKind,
    done: u64,
    status: JobStatus,
}

fn running(id: u64, done: u64) -> Fake {
    Fake {
        id,
        kind: OpKind::Copy,
        done,
        status: JobStatus::Running,
    }
}

fn add(cx: &mut TestAppContext, view: &Entity<FerriteWindow>, job: Fake) {
    view.update(cx, |view, cx| {
        let source_tab = view.browser.state().active_tab().id();
        view.browser.push_job(JobView {
            id: job.id,
            kind: job.kind,
            from: Some(at("/home/commander")),
            target: Some(at("/archive")),
            sources: 4,
            progress: Progress {
                bytes_done: job.done,
                bytes_total: 400 * MB,
                files_done: 1,
                files_total: 12,
                current: Some(at("/home/commander/Sinaia.7z")),
            },
            status: job.status,
            cancel: CancelToken::new(),
            source_tab,
        });
        cx.notify();
    });
}

fn give_speed(cx: &mut TestAppContext, view: &Entity<FerriteWindow>, id: u64, per_second: u64) {
    view.update(cx, |view, _| {
        let start = Instant::now();
        let meter = &mut view.job_ui.meter;
        meter.observe(id, 0, start);
        meter.observe(id, per_second, start + Duration::from_secs(1));
    });
}

fn key(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press(key, cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

#[derive(Debug)]
struct Shown {
    card: Option<String>,
    pill: Option<String>,
    pill_parts: [bool; 3],
}

fn shown(cx: &mut TestAppContext, window: AnyWindowHandle) -> Shown {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let card = window
            .try_find("job-card")
            .map(|card| card.label().unwrap_or_default().to_owned());
        let bar = window.within("fkeys");
        let pill = bar
            .try_find("job-pill")
            .map(|pill| pill.label().unwrap_or_default().to_owned());
        let pill_parts = ["job-pill-name", "job-pill-status", "job-pill-left"]
            .map(|part| bar.try_find(part).is_some());
        Shown {
            card,
            pill,
            pill_parts,
        }
    })
    .expect("window")
}

fn label(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) -> String {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find(id).label().unwrap_or_default().to_owned()
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_card_appears_while_a_job_runs(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    assert!(shown(cx, window).card.is_none());
    add(cx, &view, running(1, 100 * MB));
    let now = shown(cx, window);
    let card = now.card.expect("a card");
    assert!(card.starts_with("Copying 4 items, 25%"), "{card}");
    assert!(now.pill.is_none());
    assert_eq!(
        label(cx, window, ("job-route", 1u64)),
        "commander → /archive"
    );
    assert_eq!(label(cx, window, ("job-current", 1u64)), "Sinaia.7z");
    assert_eq!(
        label(cx, window, ("job-bytes", 1u64)),
        "100.0 MB / 400.0 MB"
    );
}

#[gpui_kit::test]
fn ctrl_j_shrinks_the_card_into_the_pill_and_grows_it_back(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "ctrl-j");
    let small = shown(cx, window);
    assert!(small.card.is_none());
    let pill = small.pill.expect("a pill");
    assert!(pill.starts_with("Copy 4 items, 25%"), "{pill}");
    key(cx, window, "ctrl-j");
    let big = shown(cx, window);
    assert!(big.card.is_some());
    assert!(big.pill.is_none());
}

#[gpui_kit::test]
fn the_chevron_shrinks_the_card(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    add(cx, &view, running(1, 100 * MB));
    click(cx, window, "job-shrink");
    let small = shown(cx, window);
    assert!(small.card.is_none());
    assert!(small.pill.is_some());
}

#[gpui_kit::test]
fn a_click_on_the_pill_grows_the_card_back(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "ctrl-j");
    click(cx, window, "job-pill");
    let big = shown(cx, window);
    assert!(big.card.is_some());
    assert!(big.pill.is_none());
}

#[gpui_kit::test]
fn ctrl_j_without_a_job_shows_nothing(cx: &mut TestAppContext) {
    let (window, _) = open_at(cx, 1280.);
    key(cx, window, "ctrl-j");
    let now = shown(cx, window);
    assert!(now.card.is_none());
    assert!(now.pill.is_none());
}

#[gpui_kit::test]
fn two_jobs_show_a_count_and_combined_progress(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    add(cx, &view, running(1, 100 * MB));
    add(
        cx,
        &view,
        Fake {
            kind: OpKind::Move,
            ..running(2, 0)
        },
    );
    let card = shown(cx, window).card.expect("a card");
    assert!(card.starts_with("2 jobs, 12%"), "{card}");
    assert!(label(cx, window, ("job-row", 1u64)).starts_with("Copy 4 items, 25%"));
    assert!(label(cx, window, ("job-row", 2u64)).starts_with("Move 4 items, 0%"));
    key(cx, window, "ctrl-j");
    let pill = shown(cx, window).pill.expect("a pill");
    assert!(pill.starts_with("2 jobs, 12%"), "{pill}");
}

#[gpui_kit::test]
fn narrow_windows_drop_the_name_then_the_time_left(cx: &mut TestAppContext) {
    let cases = [
        (1280., [true, true, true]),
        (1050., [false, true, true]),
        (900., [false, true, false]),
    ];
    for (width, parts) in cases {
        let (window, view) = open_at(cx, width);
        add(cx, &view, running(1, 100 * MB));
        give_speed(cx, &view, 1, 10 * MB);
        key(cx, window, "ctrl-j");
        assert_eq!(shown(cx, window).pill_parts, parts, "{width}");
    }
}

#[gpui_kit::test]
fn cancel_asks_the_engine_to_stop_the_job(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    add(cx, &view, running(1, 100 * MB));
    click(cx, window, ("job-cancel", 1u64));
    let stopped = view.read_with(cx, |view, _| view.browser.jobs()[0].cancel.is_cancelled());
    assert!(stopped);
}

#[gpui_kit::test]
fn cancel_in_a_list_of_jobs_stops_only_that_job(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    add(cx, &view, running(1, 100 * MB));
    add(cx, &view, running(2, 0));
    click(cx, window, ("job-cancel", 2u64));
    let stopped = view.read_with(cx, |view, _| {
        view.browser
            .jobs()
            .iter()
            .map(|job| job.cancel.is_cancelled())
            .collect::<Vec<_>>()
    });
    assert_eq!(stopped, [false, true]);
}

#[gpui_kit::test]
fn done_shows_briefly_then_the_card_goes_away(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    press(cx, window, &view, "f5");
    press(cx, window, &view, "enter");
    let card = shown(cx, window).card.expect("a card");
    assert!(card.starts_with("Copied 1 item, Done"), "{card}");
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(shown(cx, window).card.is_some());
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    assert!(shown(cx, window).card.is_none());
    assert!(view.read_with(cx, |view, _| view.browser.jobs().is_empty()));
}

#[gpui_kit::test]
fn an_error_stays_until_it_is_dismissed(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    add(
        cx,
        &view,
        Fake {
            status: JobStatus::Failed("Access denied".to_owned()),
            ..running(1, 100 * MB)
        },
    );
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert!(shown(cx, window).card.is_some());
    assert_eq!(label(cx, window, ("job-message", 1u64)), "Access denied");
    click(cx, window, ("job-dismiss", 1u64));
    assert!(shown(cx, window).card.is_none());
    assert!(view.read_with(cx, |view, _| view.browser.jobs().is_empty()));
}
