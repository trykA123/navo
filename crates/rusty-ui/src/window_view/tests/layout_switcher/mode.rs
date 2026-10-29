use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::{AppState, Command, Direction, PaneId, apply};

use super::super::{fake, open_options, options_for, press, two_panes};
use crate::window_view::FerriteWindow;

type Rig = (AnyWindowHandle, Entity<FerriteWindow>);

fn open_state(cx: &mut TestAppContext, build: impl FnOnce(&rusty_fs::FakeFs) -> AppState) -> Rig {
    let fs = fake();
    let state = build(&fs);
    let rig = open_options(cx, options_for(fs, state));
    press(cx, rig.0, &rig.1, "ctrl-l");
    rig
}

fn split(state: &mut AppState, direction: Direction) {
    apply(state, &Command::LayoutSplit { direction }).expect("split");
}

fn numbered(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<PaneId> {
    view.read_with(cx, |view, _| view.tiles_layout().numbered())
}

fn root_ratio(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> u16 {
    view.read_with(cx, |view, _| view.tiles_layout().splits()[0].ratio)
}

fn is_open(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> bool {
    view.read_with(cx, |view, _| view.tiles.switcher.is_some())
}

fn tab_count(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> usize {
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let window = state.window(state.focused_window()).expect("window");
        window.panes().iter().map(|pane| pane.tabs().len()).sum()
    })
}

fn focused(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> PaneId {
    view.read_with(cx, |view, _| view.browser.state().focused_pane())
}

#[gpui_kit::test]
fn v_then_s_gives_three_panes_and_the_switcher_stays_open(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, |fs| AppState::new(fs.location("home")));
    press(cx, window, &view, "v");
    press(cx, window, &view, "s");
    assert_eq!(numbered(cx, &view).len(), 3);
    assert!(is_open(cx, &view));
    press(cx, window, &view, "4");
    assert_eq!(numbered(cx, &view).len(), 4);
}

#[gpui_kit::test]
fn shift_right_swaps_pane_one_and_two(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, |fs| {
        let mut state = two_panes(fs);
        state.focus_number(1).expect("first pane");
        state
    });
    let before = numbered(cx, &view);
    press(cx, window, &view, "shift-right");
    assert_eq!(numbered(cx, &view), vec![before[1], before[0]]);
    assert_eq!(focused(cx, &view), before[0]);
    assert!(is_open(cx, &view));
}

#[gpui_kit::test]
fn ctrl_right_moves_the_root_divider_by_five_percent_and_equals_evens(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, |fs| {
        let mut state = two_panes(fs);
        state.focus_number(1).expect("first pane");
        state
    });
    assert_eq!(root_ratio(cx, &view), 500);
    press(cx, window, &view, "ctrl-right");
    assert_eq!(root_ratio(cx, &view), 550);
    press(cx, window, &view, "ctrl-right");
    assert_eq!(root_ratio(cx, &view), 600);
    assert!(is_open(cx, &view));
    press(cx, window, &view, "=");
    assert_eq!(root_ratio(cx, &view), 500);
    assert!(is_open(cx, &view));
}

#[gpui_kit::test]
fn x_closes_the_focused_pane_and_keeps_its_tabs(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, |fs| {
        let mut state = two_panes(fs);
        state.focus_number(2).expect("second pane");
        let location = Some(fs.location("dest"));
        apply(&mut state, &Command::PaneOpenTab { location }).expect("tab");
        state
    });
    let tabs = tab_count(cx, &view);
    assert_eq!(tabs, 3);
    press(cx, window, &view, "x");
    assert_eq!(numbered(cx, &view).len(), 1);
    assert_eq!(tab_count(cx, &view), tabs);
    assert!(is_open(cx, &view));
}

#[gpui_kit::test]
fn a_fifth_split_shows_the_limit_and_keeps_four_panes(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, |fs| {
        let mut state = two_panes(fs);
        split(&mut state, Direction::Down);
        split(&mut state, Direction::Down);
        state
    });
    assert_eq!(numbered(cx, &view).len(), 4);
    press(cx, window, &view, "v");
    assert_eq!(numbered(cx, &view).len(), 4);
    assert!(is_open(cx, &view));
    let notice = view.read_with(cx, |view, _| view.status_notice().map(|n| n.text));
    assert_eq!(
        notice.as_deref(),
        Some(rusty_l10n::tr("layout-too-many-panes").as_str())
    );
}

#[gpui_kit::test]
fn plain_right_still_moves_the_thumbnail_focus(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, two_panes);
    let panes = numbered(cx, &view);
    press(cx, window, &view, "right");
    assert_eq!(numbered(cx, &view), panes);
    assert!(is_open(cx, &view));
    press(cx, window, &view, "enter");
    assert_eq!(
        view.read_with(cx, |view, _| view.tiles_layout().to_tiles()),
        rusty_core::Tiles::builtin()[1].tiles
    );
    assert!(!is_open(cx, &view));
}

#[gpui_kit::test]
fn tab_moves_pane_focus_to_the_target_and_keys_stay_in_layout_mode(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, |fs| {
        let mut state = two_panes(fs);
        state.focus_number(1).expect("first pane");
        state
    });
    let panes = numbered(cx, &view);
    press(cx, window, &view, "tab");
    assert_eq!(focused(cx, &view), panes[1]);
    assert!(is_open(cx, &view));
    press(cx, window, &view, "ctrl-shift-1");
    assert_eq!(focused(cx, &view), panes[0]);
    press(cx, window, &view, "s");
    assert_eq!(numbered(cx, &view).len(), 3);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("layout-switcher").visible());
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_new_pane_fades_in_unless_motion_is_reduced(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, two_panes);
    press(cx, window, &view, "s");
    assert_eq!(view.read_with(cx, |view, _| view.tiles.fades.count()), 1);
    press(cx, window, &view, "=");
    assert_eq!(view.read_with(cx, |view, _| view.tiles.fades.count()), 1);
    cx.update(|cx| cx.set_reduce_motion(true));
    press(cx, window, &view, "v");
    assert_eq!(numbered(cx, &view).len(), 4);
    assert_eq!(view.read_with(cx, |view, _| view.tiles.fades.count()), 1);
}

#[gpui_kit::test]
fn enter_after_a_mode_edit_keeps_the_edited_layout(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, two_panes);
    press(cx, window, &view, "v");
    let edited = view.read_with(cx, |view, _| view.tiles_layout().to_tiles());
    assert_eq!(numbered(cx, &view).len(), 3);
    press(cx, window, &view, "enter");
    assert!(!is_open(cx, &view));
    assert_eq!(numbered(cx, &view).len(), 3);
    assert_eq!(
        view.read_with(cx, |view, _| view.tiles_layout().to_tiles()),
        edited
    );
}

#[gpui_kit::test]
fn an_arrow_after_a_mode_edit_lets_enter_apply_the_focused_preset(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, two_panes);
    press(cx, window, &view, "v");
    press(cx, window, &view, "right");
    press(cx, window, &view, "left");
    press(cx, window, &view, "enter");
    assert!(!is_open(cx, &view));
    assert_eq!(
        view.read_with(cx, |view, _| view.tiles_layout().to_tiles()),
        rusty_core::Tiles::builtin()[0].tiles
    );
}

#[cfg(target_os = "linux")]
#[gpui_kit::test]
fn linux_shifted_digit_keystroke_jumps_to_the_pane(cx: &mut TestAppContext) {
    let (window, view) = open_state(cx, |fs| {
        let mut state = two_panes(fs);
        state.focus_number(2).expect("second pane");
        state
    });
    let panes = numbered(cx, &view);
    for (key, pane) in [("ctrl-!", 0), ("ctrl-@", 1), ("ctrl-!", 0), ("ctrl-\"", 1)] {
        press(cx, window, &view, key);
        assert_eq!(focused(cx, &view), panes[pane], "{key}");
    }
    press(cx, window, &view, "escape");
    assert!(!is_open(cx, &view));
    for (key, pane) in [("ctrl-!", 0), ("ctrl-@", 1)] {
        press(cx, window, &view, key);
        assert_eq!(focused(cx, &view), panes[pane], "{key}");
    }
}
