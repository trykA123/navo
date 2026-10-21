use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, ElementId, Entity, Pixels, TestAppContext, px,
};
use rusty_core::OpKind;

use super::jobs::{Fake, MB, add, key, running};
use super::{fake, home_beside_dest, open_options_sized, options_for, press, two_panes};
use crate::activity::ActivityId;
use crate::activity_strip::key_of;
use crate::browser::JobStatus;
use crate::command_center::Indexing;
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::FerriteWindow;

fn open_plain(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_wide(cx, 1280.)
}

fn open_wide(cx: &mut TestAppContext, width: f32) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let mut options = options_for(fs, state);
    options.settings.values = Values::default();
    open_options_sized(cx, options, px(width))
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

fn in_pane(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) -> bool {
    find(cx, window, |window, cx| {
        let view = view.read(cx);
        let pane = view.browser.state().focused_pane();
        view.focus
            .get(&pane)
            .is_some_and(|focus| focus.is_focused(window))
    })
}

fn done(id: u64) -> Fake {
    Fake {
        status: JobStatus::Done,
        ..running(id, 400 * MB)
    }
}

fn bounds(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
) -> Option<Bounds<Pixels>> {
    let id = id.into();
    find(cx, window, |window, _| {
        window.try_find(id).map(|found| found.bounds())
    })
}

#[gpui_kit::test]
fn show_in_the_drawer_closes_it_and_the_card_is_on_top(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    add(cx, &view, failed(2));
    key(cx, window, "alt-j");
    assert!(in_drawer(cx, window, ("activity-show", 2u64)));
    key(cx, window, "enter");
    assert!(!has(cx, window, "activity-drawer"));
    let card = find(cx, window, |window, _| {
        window
            .try_find("job-card")
            .is_some_and(|card| card.visible())
    });
    assert!(card);
    assert!(in_pane(cx, window, &view));
}

#[gpui_kit::test]
fn escape_from_the_line_returns_to_the_pane(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "alt-j");
    assert!(!in_pane(cx, window, &view));
    key(cx, window, "escape");
    assert!(in_pane(cx, window, &view));
}

#[gpui_kit::test]
fn escape_from_the_drawer_returns_to_the_viewer(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let file = fs.location("home/a.txt");
    let (window, view) = open_options_sized(cx, options_for(fs, state), px(1280.));
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.view_location(file, window, cx));
    })
    .expect("window");
    cx.run_until_parked();
    add(cx, &view, running(1, 100 * MB));
    add(cx, &view, running(2, 0));
    let in_viewer = |cx: &mut TestAppContext| {
        find(cx, window, |window, cx| {
            let view = view.read(cx);
            let viewer = view.viewer.as_ref().expect("viewer");
            viewer.focus_target(cx).is_focused(window)
        })
    };
    assert!(in_viewer(cx));
    click(cx, window, "activity-all");
    assert!(in_drawer(cx, window, ("activity-pause", 2u64)));
    key(cx, window, "escape");
    assert!(!has(cx, window, "activity-drawer"));
    assert!(in_viewer(cx));
}

#[gpui_kit::test]
fn delete_and_space_in_the_strip_leave_files_dialogs_and_marks_alone(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options_sized(cx, options_for(fs.clone(), state), px(1280.));
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "alt-j");
    key(cx, window, "space");
    key(cx, window, "delete");
    assert_eq!(pause_asked(cx, &view), [true]);
    assert_eq!(cancelled(cx, &view), [true]);
    let (pending, marked) = view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        (view.browser.pending().is_some(), tab.selection().marked())
    });
    assert!(!pending);
    assert!(marked.is_empty(), "{marked:?}");
    assert!(fs.recycled().is_empty());
}

#[gpui_kit::test]
fn a_done_item_stays_while_the_strip_has_focus(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, done(1));
    key(cx, window, "alt-j");
    advance(cx, Duration::from_secs(20));
    assert!(has(cx, window, "activity-strip"));
    key(cx, window, "escape");
    advance(cx, Duration::from_secs(9));
    assert!(!has(cx, window, "activity-strip"));
    assert!(in_pane(cx, window, &view));
}

#[gpui_kit::test]
fn alt_j_on_an_item_without_buttons_rings_the_band(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, done(1));
    assert!(!selected(cx, window, "activity-band"));
    key(cx, window, "alt-j");
    assert!(selected(cx, window, "activity-band"));
}

#[gpui_kit::test]
fn a_job_ending_while_the_drawer_is_open_settles_the_selection(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    add(cx, &view, running(2, 0));
    key(cx, window, "alt-j");
    assert!(in_drawer(cx, window, ("activity-pause", 2u64)));
    view.update(cx, |view, cx| {
        view.browser.jobs_mut()[1].status = JobStatus::Done;
        view.refresh_activity(cx);
    });
    assert!(has(cx, window, "activity-drawer"));
    assert!(in_drawer(cx, window, ("activity-pause", 1u64)));
    key(cx, window, "space");
    assert_eq!(pause_asked(cx, &view), [true, false]);
}

#[gpui_kit::test]
fn delete_and_space_on_the_all_button_do_nothing(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "alt-j");
    add(cx, &view, running(2, 0));
    key(cx, window, "left");
    assert!(selected(cx, window, "activity-all"));
    key(cx, window, "delete");
    key(cx, window, "space");
    assert_eq!(cancelled(cx, &view), [false, false]);
    assert_eq!(pause_asked(cx, &view), [false, false]);
}

#[gpui_kit::test]
fn show_and_dismiss_keys_show_only_while_the_strip_has_focus(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    add(cx, &view, failed(1));
    assert_eq!(label(cx, window, ("activity-show", 1u64)), "Show");
    assert_eq!(label(cx, window, ("activity-dismiss", 1u64)), "Dismiss");
    key(cx, window, "alt-j");
    assert_ne!(label(cx, window, ("activity-show", 1u64)), "Show");
    assert_ne!(label(cx, window, ("activity-dismiss", 1u64)), "Dismiss");
}

fn centre_offset(cx: &mut TestAppContext, window: AnyWindowHandle, id: ElementId) -> f32 {
    let band = bounds(cx, window, "activity-band").expect("band");
    let shown = bounds(cx, window, id).expect("element");
    f32::from(shown.center().y - band.center().y).abs()
}

#[gpui_kit::test]
fn with_reduced_motion_nothing_travels(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    cx.update(|cx| cx.set_reduce_motion(true));
    add(cx, &view, running(1, 100 * MB));
    add(cx, &view, running(2, 0));
    let title = centre_offset(cx, window, ("activity-title", 2u64).into());
    let chip = centre_offset(cx, window, ("activity-chip", 1u64).into());
    assert!(title < 0.5, "{title}");
    assert!(chip < 0.5, "{chip}");
}

#[gpui_kit::test]
fn without_reduced_motion_a_new_line_rises(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    cx.update(|cx| {
        let mut theme = crate::tokens::current(cx).clone();
        theme.motion.base = Duration::from_secs(600);
        cx.set_global(theme);
    });
    add(cx, &view, running(1, 100 * MB));
    std::thread::sleep(Duration::from_millis(250));
    let title = centre_offset(cx, window, ("activity-title", 1u64).into());
    assert!(title > 3., "{title}");
}

fn six_jobs(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) {
    for id in 1..=6 {
        add(cx, view, running(id, id * 10 * MB));
    }
}

fn assert_primary_fits(cx: &mut TestAppContext, window: AnyWindowHandle) {
    let band = bounds(cx, window, "activity-band").expect("band");
    let all = bounds(cx, window, "activity-all").expect("all");
    let pause = bounds(cx, window, ("activity-pause", 6u64)).expect("pause");
    let cancel = bounds(cx, window, ("activity-cancel", 6u64)).expect("cancel");
    assert!(pause.left() > band.left(), "{pause:?} {band:?}");
    assert!(cancel.right() <= all.left(), "{cancel:?} {all:?}");
    assert!(all.right() <= band.right(), "{all:?} {band:?}");
    for id in 1..=5u64 {
        let chip = bounds(cx, window, ("activity-chip", id)).expect("chip");
        let is_shown = chip.left() < band.right() && chip.size.width > px(0.);
        assert!(!is_shown || chip.left() >= cancel.right(), "{id} {chip:?}");
    }
}

#[gpui_kit::test]
fn many_items_keep_the_primary_whole_at_1280(cx: &mut TestAppContext) {
    let (window, view) = open_wide(cx, 1280.);
    six_jobs(cx, &view);
    advance(cx, Duration::from_secs(1));
    assert_primary_fits(cx, window);
    let title = bounds(cx, window, ("activity-title", 6u64)).expect("title");
    assert!(title.size.width > px(100.), "{title:?}");
}

#[gpui_kit::test]
fn many_items_keep_the_primary_whole_at_390(cx: &mut TestAppContext) {
    let (window, view) = open_wide(cx, 390.);
    six_jobs(cx, &view);
    advance(cx, Duration::from_secs(1));
    assert_primary_fits(cx, window);
}

#[gpui_kit::test]
fn chips_that_are_cut_off_are_not_stops(cx: &mut TestAppContext) {
    let (window, view) = open_wide(cx, 390.);
    add(cx, &view, running(1, 100 * MB));
    key(cx, window, "alt-j");
    for id in 2..=6 {
        add(cx, &view, running(id, id * 10 * MB));
    }
    advance(cx, Duration::from_secs(1));
    let band = bounds(cx, window, "activity-band").expect("band");
    let all = bounds(cx, window, "activity-all").expect("all");
    for _ in 0..8 {
        key(cx, window, "right");
        for id in 1..=5u64 {
            if selected(cx, window, ("activity-chip", id)) {
                let chip = bounds(cx, window, ("activity-chip", id)).expect("chip");
                assert!(chip.right() <= all.left(), "{id} {chip:?} {all:?}");
                assert!(chip.left() >= band.left(), "{id} {chip:?}");
                assert!(chip.bottom() <= band.bottom(), "{id} {chip:?} {band:?}");
                assert!(chip.size.width >= px(40.), "{id} {chip:?}");
            }
        }
    }
}

#[gpui_kit::test]
fn down_in_a_long_drawer_keeps_the_selected_row_in_view(cx: &mut TestAppContext) {
    let (window, view) = open_plain(cx);
    for id in 1..=12 {
        add(cx, &view, running(id, id * MB));
    }
    key(cx, window, "alt-j");
    for _ in 0..11 {
        key(cx, window, "down");
    }
    assert!(in_drawer(cx, window, ("activity-pause", 1u64)));
    let drawer = bounds(cx, window, "activity-drawer").expect("drawer");
    let row = bounds(cx, window, ("activity-row", 1u64)).expect("row");
    assert!(row.bottom() <= drawer.bottom(), "{row:?} {drawer:?}");
    assert!(row.top() >= drawer.top(), "{row:?} {drawer:?}");
}

#[derive(Default)]
struct IndexStatus {
    value: Mutex<Option<Indexing>>,
    polls: AtomicUsize,
    searches: AtomicUsize,
}

fn indexing(count: u64) -> Indexing {
    Indexing {
        root: "C:".into(),
        count,
    }
}

fn open_index(
    cx: &mut TestAppContext,
    off: bool,
) -> (AnyWindowHandle, Entity<FerriteWindow>, Arc<IndexStatus>) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let mut options = options_for(fs, state);
    options.settings.values = Values::default();
    if off {
        options.settings.values = options.settings.values.with(&Change {
            field: Field::SearchIndex,
            value: Value::Pick(3),
        });
    }
    let status = Arc::new(IndexStatus::default());
    *status.value.lock().unwrap() = Some(indexing(12_345));
    let polls = status.clone();
    options.search.index.status = Arc::new(move || {
        polls.polls.fetch_add(1, Ordering::SeqCst);
        polls.value.lock().unwrap().clone()
    });
    let searches = status.clone();
    options.search.index.host = Arc::new(move |_, _, _, _| {
        searches.searches.fetch_add(1, Ordering::SeqCst);
        Default::default()
    });
    let pipe_searches = status.clone();
    options.search.index.pipe =
        crate::command_center::index_source::PipeQueries::new(Arc::new(move |_, _, _| {
            pipe_searches.searches.fetch_add(1, Ordering::SeqCst);
            Ok(Default::default())
        }));
    let (window, view) = open_options_sized(cx, options, px(1280.));
    advance(cx, Duration::from_millis(500));
    (window, view, status)
}

fn index_title(cx: &mut TestAppContext, window: AnyWindowHandle) -> String {
    label(
        cx,
        window,
        ("activity-title", key_of(ActivityId::Background(0))),
    )
}

#[gpui_kit::test]
fn index_activity_appears_while_indexing(cx: &mut TestAppContext) {
    let (window, view, _) = open_index(cx, false);
    assert!(has(cx, window, "activity-strip"));
    assert_eq!(index_title(cx, window), indexing(12_345).text().as_ref());
    view.read_with(cx, |view, _| {
        let item = view.job_ui.activity.primary().unwrap();
        assert_eq!(item.id, ActivityId::Background(0));
        assert_eq!(item.progress, None);
    });
}

#[gpui_kit::test]
fn index_activity_updates_the_count(cx: &mut TestAppContext) {
    let (window, _, status) = open_index(cx, false);
    *status.value.lock().unwrap() = Some(indexing(24_690));
    advance(cx, Duration::from_millis(499));
    assert_eq!(index_title(cx, window), indexing(12_345).text().as_ref());
    advance(cx, Duration::from_millis(1));
    assert_eq!(index_title(cx, window), indexing(24_690).text().as_ref());
}

#[gpui_kit::test]
fn index_activity_ends_when_status_is_none(cx: &mut TestAppContext) {
    let (window, view, status) = open_index(cx, false);
    *status.value.lock().unwrap() = None;
    advance(cx, Duration::from_millis(500));
    assert!(!has(cx, window, "activity-strip"));
    assert!(view.read_with(cx, |view, _| view.job_ui.activity.is_empty()));
}

#[gpui_kit::test]
fn index_activity_shows_nothing_when_off(cx: &mut TestAppContext) {
    let (window, _, status) = open_index(cx, true);
    advance(cx, Duration::from_millis(500));
    assert!(!has(cx, window, "activity-strip"));
    assert_eq!(status.polls.load(Ordering::SeqCst), 0);
    assert_eq!(status.searches.load(Ordering::SeqCst), 0);
}

#[gpui_kit::test]
fn index_activity_polls_status_twice_a_second_without_searching(cx: &mut TestAppContext) {
    let (_, _, status) = open_index(cx, false);
    let before = status.polls.load(Ordering::SeqCst);
    assert!(before > 0);
    for _ in 0..4 {
        advance(cx, Duration::from_millis(500));
    }
    assert_eq!(status.polls.load(Ordering::SeqCst) - before, 4);
    assert_eq!(status.searches.load(Ordering::SeqCst), 0);
}
