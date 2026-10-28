use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use rusty_core::Tiles;

use super::{fake, open_options, options_for, press, two_panes};

#[gpui_kit::test]
fn ctrl_l_opens_the_switcher_and_four_applies_two_by_two(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    press(cx, window, &view, "ctrl-l");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("layout-switcher").visible());
        assert_eq!(window.find(("layout-preset", 1usize)).checked(), Some(true));
        assert_eq!(
            window.find(("layout-preset", 4usize)).checked(),
            Some(false)
        );
    })
    .expect("window");
    assert!(view.read_with(cx, |view, _| view.path_edit.is_none()));
    press(cx, window, &view, "4");
    assert!(view.read_with(cx, |view, _| view.tiles.switcher.is_none()));
    assert_eq!(
        view.read_with(cx, |view, _| view.tiles_layout().to_tiles()),
        Tiles::builtin()[3].tiles
    );
}

#[gpui_kit::test]
fn escape_preserves_the_layout_and_returns_focus_to_the_file_list(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    let before = view.read_with(cx, |view, _| view.tiles_layout());
    press(cx, window, &view, "ctrl-l");
    press(cx, window, &view, "right");
    press(cx, window, &view, "escape");
    assert!(view.read_with(cx, |view, _| view.tiles.switcher.is_none()));
    assert_eq!(view.read_with(cx, |view, _| view.tiles_layout()), before);
    press(cx, window, &view, "alt-d");
    assert!(view.read_with(cx, |view, _| view.path_edit.is_some()));
}

#[gpui_kit::test]
fn arrows_then_enter_apply_the_focused_thumbnail_before_file_list_keys(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    press(cx, window, &view, "ctrl-l");
    for key in ["right", "down", "left", "up", "right", "enter"] {
        press(cx, window, &view, key);
    }
    assert!(view.read_with(cx, |view, _| view.tiles.switcher.is_none()));
    assert_eq!(
        view.read_with(cx, |view, _| view.tiles_layout().to_tiles()),
        Tiles::builtin()[1].tiles
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.browser.state().focused_pane()),
        pane
    );
}

#[gpui_kit::test]
fn alt_d_opens_the_path_editor(cx: &mut TestAppContext) {
    let fs = fake();
    let state = two_panes(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    press(cx, window, &view, "alt-d");
    assert!(view.read_with(cx, |view, _| view.path_edit.is_some()));
    assert!(view.read_with(cx, |view, _| view.tiles.switcher.is_none()));
}
