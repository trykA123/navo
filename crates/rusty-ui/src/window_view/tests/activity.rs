use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext, px};
use rusty_core::OpKind;

use super::jobs::{Fake, MB, add, key, running};
use super::{fake, home_beside_dest, open_options_sized, options_for, press};
use crate::browser::JobStatus;
use crate::settings::Values;
use crate::window_view::FerriteWindow;

fn open_plain(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let mut options = options_for(fs, state);
    options.settings.values = Values::default();
    open_options_sized(cx, options, px(1280.))
}

fn failed(id: u64) -> Fake {
    Fake {
        status: JobStatus::Failed("Access denied".to_owned()),
        ..running(id, 100 * MB)
    }
}

fn find<T>(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    read: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::App) -> T,
) -> T {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        read(window, cx)
    })
    .expect("window")
}

fn has(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    find(cx, window, |window, _| window.try_find(id).is_some())
}

fn label(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) -> String {
    let id = id.into();
    find(cx, window, |window, _| {
        window.find(id).label().unwrap_or_default().to_owned()
    })
}

fn selected(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    find(cx, window, |window, _| {
        window.find(id).selected() == Some(true)
    })
}

fn in_drawer(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) -> bool {
    let id = id.into();
    find(cx, window, |window, _| {
        let drawer = window.within("activity-drawer");
        drawer
            .try_find(id)
            .is_some_and(|found| found.selected() == Some(true))
    })
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: impl Into<ElementId>) {
    let id = id.into();
    find(cx, window, |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

fn advance(cx: &mut TestAppContext, by: Duration) {
    cx.executor().advance_clock(by);
    cx.run_until_parked();
}

fn pause_asked(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<bool> {
    view.read_with(cx, |view, _| {
        let jobs = view.browser.jobs();
        jobs.iter().map(|job| job.cancel.is_paused()).collect()
    })
}

fn cancelled(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<bool> {
    view.read_with(cx, |view, _| {
        let jobs = view.browser.jobs();
        jobs.iter().map(|job| job.cancel.is_cancelled()).collect()
    })
}

#[gpui_kit::test]
fn with_default_settings_the_bottom_row_is_gone_when_idle(cx: &mut TestAppContext) {
    let (window, _) = open_plain(cx);
    assert!(!has(cx, window, "activity-strip"));
    assert!(!has(cx, window, "fkeys"));
}

#[gpui_kit::test]
fn a_running_copy_shows_its_line_with_pause_and_cancel(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    assert!(has(cx, window, "activity-strip"));
    let title = label(cx, window, ("activity-title", 1u64));
    assert_eq!(title, "Copying 12 files to /archive");
    assert_eq!(label(cx, window, ("activity-percent", 1u64)), "25%");
    assert!(has(cx, window, ("activity-pause", 1u64)));
    assert!(has(cx, window, ("activity-cancel", 1u64)));
    assert!(!has(cx, window, "activity-all"));
}

#[gpui_kit::test]
fn the_row_fills_as_the_job_progresses(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    let (fill, row) = find(cx, window, |window, _| {
        let row = window.find("activity-strip").bounds();
        (window.find("activity-fill").bounds(), row)
    });
    let share = fill.size.width / (row.size.width - px(16.));
    assert!((share - 0.25).abs() < 0.02, "{share}");
}

#[gpui_kit::test]
fn pause_and_cancel_buttons_reach_the_job(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    click(cx, window, ("activity-pause", 1u64));
    assert_eq!(pause_asked(cx, &view), [true]);
    assert!(has(cx, window, ("activity-resume", 1u64)));
    click(cx, window, ("activity-resume", 1u64));
    assert_eq!(pause_asked(cx, &view), [false]);
    click(cx, window, ("activity-cancel", 1u64));
    assert_eq!(cancelled(cx, &view), [true]);
}

#[gpui_kit::test]
fn a_finished_copy_reads_done_then_the_row_leaves(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    press(cx, window, &view, "f5");
    press(cx, window, &view, "enter");
    let title = label(cx, window, ("activity-title", 1u64));
    assert!(title.starts_with("Copied 1 item to "), "{title}");
    assert!(!has(cx, window, ("activity-pause", 1u64)));
    advance(cx, Duration::from_secs(7));
    assert!(has(cx, window, "activity-strip"));
    advance(cx, Duration::from_millis(1100));
    assert!(!has(cx, window, "activity-strip"));
    advance(cx, Duration::from_millis(200));
    assert!(!has(cx, window, "activity-strip-leaving"));
}

#[gpui_kit::test]
fn hovering_the_row_keeps_a_done_item(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    press(cx, window, &view, "f5");
    press(cx, window, &view, "enter");
    find(cx, window, |window, cx| window.hover("activity-strip", cx));
    advance(cx, Duration::from_secs(20));
    assert!(has(cx, window, "activity-strip"));
}

#[gpui_kit::test]
fn a_failure_shows_and_dismisses(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, failed(1));
    assert!(!has(cx, window, "job-card"));
    let title = label(cx, window, ("activity-title", 1u64));
    assert_eq!(title, "11 files could not be copied");
    click(cx, window, ("activity-show", 1u64));
    assert!(has(cx, window, "job-card"));
    click(cx, window, ("activity-dismiss", 1u64));
    assert!(!has(cx, window, "activity-strip"));
    assert!(view.read_with(cx, |view, _| view.browser.jobs().is_empty()));
}

#[gpui_kit::test]
fn other_items_wait_as_chips_and_a_click_leads_with_one(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    let moving = Fake {
        kind: OpKind::Move,
        ..running(2, 0)
    };
    add(cx, &view, moving);
    assert_eq!(label(cx, window, ("activity-chip", 1u64)), "Copy 25%");
    assert_eq!(label(cx, window, "activity-all"), "All 2 Alt+J");
    click(cx, window, ("activity-chip", 1u64));
    let title = label(cx, window, ("activity-title", 1u64));
    assert_eq!(title, "Copying 12 files to /archive");
    assert!(has(cx, window, ("activity-chip", 2u64)));
}

#[gpui_kit::test]
fn the_job_pill_is_gone(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    assert!(!has(cx, window, "job-pill"));
    assert!(!has(cx, window, "job-card"));
    assert!(has(cx, window, "activity-strip"));
}

#[gpui_kit::test]
fn with_the_fkey_bar_on_the_strip_is_a_second_row(cx: &mut TestAppContext) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_options_sized(cx, options_for(fs, state), px(1280.));
    cx.update(|cx| cx.set_reduce_motion(true));
    add(cx, &view, running(1, 100 * MB));
    let (strip, keys) = find(cx, window, |window, _| {
        let strip = window.find("activity-strip").bounds();
        (strip, window.find("fkeys").bounds())
    });
    assert!(strip.bottom() <= keys.top(), "{strip:?} {keys:?}");
}

#[gpui_kit::test]
fn alt_j_with_one_item_selects_the_first_button(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "alt-j");
    assert!(selected(cx, window, ("activity-pause", 1u64)));
    assert!(!has(cx, window, "activity-drawer"));
    key(cx, window, "right");
    assert!(selected(cx, window, ("activity-cancel", 1u64)));
    key(cx, window, "enter");
    assert_eq!(cancelled(cx, &view), [true]);
}

#[gpui_kit::test]
fn space_pauses_and_resumes_and_delete_cancels(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "alt-j");
    key(cx, window, "space");
    assert_eq!(pause_asked(cx, &view), [true]);
    key(cx, window, "space");
    assert_eq!(pause_asked(cx, &view), [false]);
    key(cx, window, "delete");
    assert_eq!(cancelled(cx, &view), [true]);
}

#[gpui_kit::test]
fn alt_j_with_two_items_opens_the_drawer_and_reaches_each(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    add(cx, &view, failed(2));
    key(cx, window, "alt-j");
    assert!(has(cx, window, "activity-drawer"));
    assert!(has(cx, window, ("activity-row", 1u64)));
    assert!(has(cx, window, ("activity-row", 2u64)));
    assert!(in_drawer(cx, window, ("activity-show", 2u64)));
    key(cx, window, "down");
    assert!(in_drawer(cx, window, ("activity-pause", 1u64)));
    key(cx, window, "up");
    key(cx, window, "delete");
    assert!(!has(cx, window, ("activity-row", 2u64)));
    key(cx, window, "space");
    assert_eq!(pause_asked(cx, &view), [true]);
}

#[gpui_kit::test]
fn escape_closes_the_drawer_and_returns_to_the_pane(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    add(cx, &view, failed(2));
    key(cx, window, "alt-j");
    key(cx, window, "escape");
    assert!(!has(cx, window, "activity-drawer"));
    let in_pane = find(cx, window, |window, cx| {
        let view = view.read(cx);
        let pane = view.browser.state().focused_pane();
        view.focus
            .get(&pane)
            .is_some_and(|focus| focus.is_focused(window))
    });
    assert!(in_pane);
}

#[gpui_kit::test]
fn alt_j_without_activity_is_left_alone(cx: &mut TestAppContext) {
    let (window, _) = open_plain(cx);
    key(cx, window, "alt-j");
    assert!(!has(cx, window, "activity-drawer"));
    assert!(!has(cx, window, "activity-strip"));
}

#[gpui_kit::test]
fn the_all_button_opens_the_drawer(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    add(cx, &view, running(2, 0));
    click(cx, window, "activity-all");
    assert!(has(cx, window, "activity-drawer"));
}
