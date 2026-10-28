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
fn tab_toggles_between_active_and_target_with_three_panes(cx: &mut TestAppContext) {
    let fs = fake();
    let state = three_panes(&fs);
    let start = state.focused_pane();
    let target = state
        .window(state.focused_window())
        .expect("window")
        .target()
        .expect("target");
    let (window, view) = open_options(cx, options_for(fs, state));
    press(cx, window, &view, "tab");
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.state().focused_pane()),
        target
    );
    press(cx, window, &view, "tab");
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.state().focused_pane()),
        start
    );
}

mod markers;
mod view;
