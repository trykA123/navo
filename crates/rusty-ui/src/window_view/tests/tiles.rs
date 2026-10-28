use gpui_kit::TestAppContext;
use rusty_core::{AppState, Command, Direction, apply};
use rusty_fs::FakeFs;

use super::{fake, open_options, options_for, press, two_panes};

pub(super) fn three_panes(fs: &FakeFs) -> AppState {
    let mut state = two_panes(fs);
    apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Down,
        },
    )
    .expect("third pane");
    state.focus_number(2).expect("second pane");
    state.focus_number(3).expect("third pane");
    state
}

#[gpui_kit::test]
fn tab_cycles_four_panes_in_reading_order_and_shift_tab_reverses(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = three_panes(&fs);
    apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    )
    .expect("fourth pane");
    state.focus_number(1).expect("first pane");
    let (window, view) = open_options(cx, options_for(fs, state));
    let panes = view.read_with(cx, |view, _| view.tiles_layout().numbered());
    for (key, order) in [("tab", [1, 2, 3, 0]), ("shift-tab", [3, 2, 1, 0])] {
        for index in order {
            let previous = view.read_with(cx, |view, _| view.browser.state().focused_pane());
            press(cx, window, &view, key);
            view.read_with(cx, |view, _| {
                assert_eq!(view.browser.state().focused_pane(), panes[index]);
                assert_eq!(view.browser.target_pane(), Some(previous));
            });
        }
    }
}

#[gpui_kit::test]
fn tab_and_shift_tab_still_toggle_two_panes(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let start = state.focused_pane();
    let (window, view) = open_options(cx, options_for(fs, state));
    for key in ["tab", "shift-tab"] {
        press(cx, window, &view, key);
        assert_ne!(
            view.read_with(cx, |view, _| view.browser.state().focused_pane()),
            start
        );
        press(cx, window, &view, key);
        assert_eq!(
            view.read_with(cx, |view, _| view.browser.state().focused_pane()),
            start
        );
    }
}

#[gpui_kit::test]
fn direct_close_keeps_the_closed_panes_tabs_in_the_last_remaining_pane(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = three_panes(&fs);
    apply(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(fs.location("dest")),
        },
    )
    .expect("extra tab");
    let closed = state.focused_pane();
    let moved: Vec<_> = state
        .pane(closed)
        .expect("closed pane")
        .tabs()
        .iter()
        .map(|tab| tab.id())
        .collect();
    let (window, view) = open_options(cx, options_for(fs, state));
    press(cx, window, &view, "ctrl-alt-w");
    view.read_with(cx, |view, _| {
        let panes = view.tiles_layout().numbered();
        assert_eq!(panes.len(), 2);
        let state = view.browser.state();
        assert!(state.pane(closed).is_none());
        let tabs = state.pane(panes[1]).expect("last kept pane").tabs();
        assert_eq!(
            tabs.iter().map(|tab| tab.id()).collect::<Vec<_>>()[tabs.len() - moved.len()..],
            moved
        );
        assert!(view.tiles.switcher.is_none());
    });
}

#[gpui_kit::test]
fn direct_close_refuses_the_last_pane(cx: &mut TestAppContext) {
    let fs = fake();
    let state = AppState::new(fs.location("home"));
    let (window, view) = open_options(cx, options_for(fs, state));
    let before = view.read_with(cx, |view, _| view.tiles_layout());
    press(cx, window, &view, "ctrl-alt-w");
    view.read_with(cx, |view, _| {
        assert_eq!(view.tiles_layout(), before);
        assert!(view.status_notice().is_some_and(|notice| notice.is_error));
    });
}

#[gpui_kit::test]
fn direct_splits_add_panes_to_the_right_and_below_and_refuse_a_fifth(cx: &mut TestAppContext) {
    let fs = fake();
    let state = AppState::new(fs.location("home"));
    let (window, view) = open_options(cx, options_for(fs, state));
    for (key, direction) in [
        ("ctrl-alt-v", Direction::Right),
        ("ctrl-alt-b", Direction::Down),
    ] {
        let before = view.read_with(cx, |view, _| view.tiles_layout());
        press(cx, window, &view, key);
        view.read_with(cx, |view, _| {
            let after = view.tiles_layout();
            assert_eq!(after.numbered().len(), before.numbered().len() + 1);
            assert_eq!(
                after.neighbour(before.focused(), direction),
                Some(after.focused())
            );
            assert!(view.tiles.switcher.is_none());
        });
    }
    press(cx, window, &view, "ctrl-alt-v");
    let before = view.read_with(cx, |view, _| view.tiles_layout());
    press(cx, window, &view, "ctrl-alt-b");
    view.read_with(cx, |view, _| {
        assert_eq!(view.tiles_layout(), before);
        assert!(view.status_notice().is_some_and(|notice| notice.is_error));
    });
}

mod drag_cost;
mod markers;
mod view;
