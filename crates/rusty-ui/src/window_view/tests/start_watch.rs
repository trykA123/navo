use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::super::start_watch::StartClock;
use super::super::{StartOptions, StartReport, StartWatch};
use super::{fake, open_options, options_for, two_panes};

const IDLE: Duration = Duration::from_secs(5);

#[test]
fn the_first_frame_is_not_usable_before_the_panes_are_listed() {
    let start = Instant::now();
    let mut clock = StartClock::default();
    assert!(!clock.frame(start, false));
    assert_eq!(clock.first_frame(), Some(start));
    assert_eq!(clock.first_usable(), None);
}

#[test]
fn the_first_listed_frame_is_usable_once() {
    let start = Instant::now();
    let later = start + Duration::from_millis(40);
    let mut clock = StartClock::default();
    assert!(!clock.frame(start, false));
    assert!(clock.frame(later, true));
    assert!(!clock.frame(later + Duration::from_millis(16), true));
    assert_eq!(clock.first_frame(), Some(start));
    assert_eq!(clock.first_usable(), Some(later));
}

#[test]
fn a_first_frame_that_is_already_listed_is_both() {
    let start = Instant::now();
    let mut clock = StartClock::default();
    assert!(clock.frame(start, true));
    assert_eq!(clock.first_frame(), Some(start));
    assert_eq!(clock.first_usable(), Some(start));
}

type Reports = Rc<RefCell<Vec<StartReport>>>;

fn watched(reports: &Reports) -> StartOptions {
    let fs = fake();
    let state = two_panes(&fs);
    let sink = reports.clone();
    StartOptions {
        start_watch: Some(StartWatch::new(IDLE, move |report| {
            sink.borrow_mut().push(report);
        })),
        ..options_for(fs, state)
    }
}

fn draw(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.simulate_next_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_watch_reports_once_after_the_listed_frame_and_the_idle_wait(cx: &mut TestAppContext) {
    let reports = Reports::default();
    let (window, view) = open_options(cx, watched(&reports));
    draw(cx, window);
    draw(cx, window);
    assert!(reports.borrow().is_empty(), "reported before the idle wait");
    cx.executor().advance_clock(IDLE);
    cx.run_until_parked();
    let reports = reports.borrow();
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert!(report.first_frame <= report.first_usable);
    assert_eq!(report.renderer, None);
    let labels: Vec<&str> = report.marks.iter().map(|(label, _)| *label).collect();
    assert!(labels.contains(&"window built"), "{labels:?}");
    assert!(
        !view.read_with(cx, |view, _| view.is_start_watched()),
        "the watch ends after its report"
    );
}

#[gpui_kit::test]
fn without_a_watch_nothing_is_reported(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    draw(cx, window);
    cx.executor().advance_clock(IDLE);
    cx.run_until_parked();
    assert!(!view.read_with(cx, |view, _| view.is_start_watched()));
}
