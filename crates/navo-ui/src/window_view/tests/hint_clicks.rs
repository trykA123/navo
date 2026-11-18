//! Mouse presses while the modifier-hint drawer is open or a button is held.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext,
};
use navo_fs::FakeFs;

use super::fkeys::hold;
use super::{NavoWindow, open_with, two_panes};
use crate::modifier_hints::HintsMode;
use crate::test_pointer::{dispatch, down, moved, shift, up};

fn many_files() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    for index in 0..60 {
        fs.add_file(&format!("home/file-{index:02}.txt"), 1);
    }
    Arc::new(fs)
}

fn drawer_top(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find("modifier-hints")
            .map(|drawer| drawer.bounds().top())
    })
    .expect("window")
}

/// Centre of the first visible row that lies under the open drawer.
fn covered_row(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) -> Point<Pixels> {
    let top = drawer_top(cx, window).expect("the drawer is open");
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, _| {
        (0..40_usize)
            .map(|row| {
                window
                    .within(("list", pane.get()))
                    .find(("row", row))
                    .bounds()
                    .center()
            })
            .find(|centre| centre.y > top + gpui_kit::px(20.))
            .expect("a row under the drawer")
    })
    .expect("window")
}

fn marked(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Vec<u64> {
    view.read_with(cx, |view, _| {
        view.browser.state().active_tab().selection().marked()
    })
}

/// Opens 60 files in an active window, so no test sees a stray activation change.
fn rig(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let fs = many_files();
    let state = two_panes(&fs);
    let (window, view) = open_with(cx, fs, state, Rc::default());
    hold(cx, window, Modifiers::default());
    (window, view)
}

fn wait(cx: &mut TestAppContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_shift_click_on_a_row_under_the_drawer_marks_it_and_hides_the_drawer(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, shift());
    wait(cx, 400);
    let at = covered_row(cx, window, &view);
    dispatch(cx, window, down(MouseButton::Left, at, shift()));
    assert_eq!(drawer_top(cx, window), None);
    dispatch(cx, window, up(MouseButton::Left, at, shift()));
    assert!(marked(cx, &view).len() > 1, "{:?}", marked(cx, &view));
}

#[gpui_kit::test]
fn the_drawer_stays_closed_while_a_mouse_button_is_held(cx: &mut TestAppContext) {
    let (window, _) = rig(cx);
    let size = cx
        .update_window(window, |_, window, _| window.viewport_size())
        .expect("window");
    let pressed = gpui_kit::point(size.width / 2., gpui_kit::px(120.));
    dispatch(
        cx,
        window,
        down(MouseButton::Left, pressed, Modifiers::default()),
    );
    hold(cx, window, shift());
    wait(cx, 2000);
    assert_eq!(drawer_top(cx, window), None);
    dispatch(cx, window, up(MouseButton::Left, pressed, shift()));
    hold(cx, window, Modifiers::default());
    hold(cx, window, shift());
    wait(cx, 400);
    assert!(drawer_top(cx, window).is_some());
}

fn in_list(cx: &mut TestAppContext, window: AnyWindowHandle) -> Point<Pixels> {
    let size = cx
        .update_window(window, |_, window, _| window.viewport_size())
        .expect("window");
    gpui_kit::point(size.width / 2., gpui_kit::px(120.))
}

/// Releases and holds Shift again; true when the drawer then opens.
fn drawer_opens_again(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    hold(cx, window, Modifiers::default());
    hold(cx, window, shift());
    wait(cx, 400);
    drawer_top(cx, window).is_some()
}

fn row_box(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    row: usize,
) -> gpui_kit::Bounds<Pixels> {
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("list", pane.get()))
            .find(("row", row))
            .bounds()
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_shift_click_on_a_lifted_row_lands_on_that_row(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    super::press(cx, window, &view, "end");
    hold(cx, window, shift());
    wait(cx, 400);
    let row = 58;
    let lifted = row_box(cx, window, &view, row);
    dispatch(
        cx,
        window,
        down(MouseButton::Left, lifted.center(), shift()),
    );
    assert_eq!(row_box(cx, window, &view, row), lifted);
    dispatch(cx, window, up(MouseButton::Left, lifted.center(), shift()));
    let marks = marked(cx, &view);
    assert!(marks.contains(&58) && !marks.contains(&57), "{marks:?}");
}

#[gpui_kit::test]
fn a_release_outside_the_window_lets_the_drawer_open_again(cx: &mut TestAppContext) {
    let (window, _) = rig(cx);
    let at = in_list(cx, window);
    dispatch(
        cx,
        window,
        down(MouseButton::Left, at, Modifiers::default()),
    );
    let outside = gpui_kit::point(gpui_kit::px(-40.), gpui_kit::px(-40.));
    dispatch(
        cx,
        window,
        up(MouseButton::Left, outside, Modifiers::default()),
    );
    assert!(drawer_opens_again(cx, window));
}

#[gpui_kit::test]
fn the_drawer_stays_closed_while_another_button_is_still_held(cx: &mut TestAppContext) {
    let (window, _) = rig(cx);
    let at = in_list(cx, window);
    dispatch(
        cx,
        window,
        down(MouseButton::Left, at, Modifiers::default()),
    );
    dispatch(
        cx,
        window,
        down(MouseButton::Middle, at, Modifiers::default()),
    );
    dispatch(
        cx,
        window,
        up(MouseButton::Middle, at, Modifiers::default()),
    );
    assert!(!drawer_opens_again(cx, window));
    dispatch(cx, window, up(MouseButton::Left, at, Modifiers::default()));
    assert!(drawer_opens_again(cx, window));
}

#[gpui_kit::test]
fn a_move_with_no_button_ends_a_press_whose_release_was_missed(cx: &mut TestAppContext) {
    let (window, _) = rig(cx);
    let at = in_list(cx, window);
    dispatch(
        cx,
        window,
        down(MouseButton::Left, at, Modifiers::default()),
    );
    dispatch(cx, window, moved(None, at, Modifiers::default()));
    assert!(drawer_opens_again(cx, window));
}

#[gpui_kit::test]
fn losing_activation_ends_a_held_press(cx: &mut TestAppContext) {
    let (window, _) = rig(cx);
    let at = in_list(cx, window);
    dispatch(
        cx,
        window,
        down(MouseButton::Left, at, Modifiers::default()),
    );
    let mut visual = gpui_kit::VisualTestContext::from_window(window, cx);
    visual.deactivate_window();
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(drawer_opens_again(cx, window));
}

fn turn_off_centre(cx: &mut TestAppContext, window: AnyWindowHandle) -> Point<Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find("modifier-hints-off").bounds().center()
    })
    .expect("window")
}

fn hints_mode(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> HintsMode {
    view.read_with(cx, |view, _| view.values.modifier_hints())
}

#[gpui_kit::test]
fn pressing_turn_off_and_dragging_away_keeps_the_hints_on(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, shift());
    wait(cx, 400);
    let at = turn_off_centre(cx, window);
    dispatch(cx, window, down(MouseButton::Left, at, shift()));
    assert!(
        drawer_top(cx, window).is_some(),
        "the drawer stays through the press"
    );
    let away = in_list(cx, window);
    dispatch(cx, window, moved(Some(MouseButton::Left), away, shift()));
    dispatch(cx, window, up(MouseButton::Left, away, shift()));
    assert_eq!(hints_mode(cx, &view), HintsMode::All);
}

#[gpui_kit::test]
fn a_one_frame_click_on_turn_off_turns_hints_off_and_leaves_the_rows(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, shift());
    wait(cx, 400);
    let at = turn_off_centre(cx, window);
    let before = marked(cx, &view);
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(down(MouseButton::Left, at, Modifiers::default()), cx);
        window.dispatch_event(up(MouseButton::Left, at, Modifiers::default()), cx);
    })
    .expect("window");
    wait(cx, 1000);
    assert_eq!(hints_mode(cx, &view), HintsMode::Fkeys);
    assert_eq!(marked(cx, &view), before);
}

#[gpui_kit::test]
fn releasing_shift_mid_press_keeps_turn_off_until_its_release(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    hold(cx, window, shift());
    wait(cx, 400);
    let at = turn_off_centre(cx, window);
    dispatch(cx, window, down(MouseButton::Left, at, shift()));
    hold(cx, window, Modifiers::default());
    assert!(drawer_top(cx, window).is_some(), "Turn off stays mounted");
    dispatch(cx, window, up(MouseButton::Left, at, Modifiers::default()));
    wait(cx, 1000);
    assert_eq!(hints_mode(cx, &view), HintsMode::Fkeys);
}

/// Opens the drawer over the last rows and presses Shift+Left on row 58; returns its lifted box.
fn press_lifted_row(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) -> gpui_kit::Bounds<Pixels> {
    super::press(cx, window, view, "end");
    hold(cx, window, shift());
    wait(cx, 400);
    let lifted = row_box(cx, window, view, 58);
    dispatch(
        cx,
        window,
        down(MouseButton::Left, lifted.center(), shift()),
    );
    lifted
}

#[gpui_kit::test]
fn a_middle_release_keeps_the_lift_of_a_held_left_press(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    let lifted = press_lifted_row(cx, window, &view);
    let at = lifted.center();
    dispatch(cx, window, down(MouseButton::Middle, at, shift()));
    dispatch(cx, window, up(MouseButton::Middle, at, shift()));
    assert_eq!(row_box(cx, window, &view, 58), lifted);
    dispatch(cx, window, up(MouseButton::Left, at, shift()));
    assert_ne!(row_box(cx, window, &view, 58), lifted);
}

#[gpui_kit::test]
fn losing_activation_lets_a_held_lift_go(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    let lifted = press_lifted_row(cx, window, &view);
    let mut visual = gpui_kit::VisualTestContext::from_window(window, cx);
    visual.deactivate_window();
    visual.run_until_parked();
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert_ne!(row_box(cx, window, &view, 58), lifted);
}
