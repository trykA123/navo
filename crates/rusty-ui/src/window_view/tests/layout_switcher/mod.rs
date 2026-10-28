use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::Tiles;

use super::{fake, open_options, options_for, press, two_panes};
use crate::window_view::{FerriteWindow, SettingsNews};

mod mode;
mod presets;

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

const REVIEW: &str = "row(70, pane, pane)";
const LOGS: &str = "col(40, pane, pane)";

fn with_user_presets(
    cx: &mut TestAppContext,
    presets: &[(&str, &str)],
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let state = two_panes(&fs);
    let mut options = options_for(fs, state);
    options.settings.values = options
        .settings
        .values
        .with_layout_presets(presets.iter().copied());
    open_options(cx, options)
}

fn shown(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str, n: usize) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find((id, n)).is_some()
    })
    .expect("window")
}

fn current_tiles(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Tiles {
    view.read_with(cx, |view, _| view.tiles_layout().to_tiles())
}

#[gpui_kit::test]
fn user_presets_follow_the_built_ins_and_six_applies_the_first(cx: &mut TestAppContext) {
    let (window, view) = with_user_presets(cx, &[("Review", REVIEW), ("Logs", LOGS)]);
    press(cx, window, &view, "ctrl-l");
    assert!(shown(cx, window, "layout-preset", 6));
    assert!(shown(cx, window, "layout-preset", 7));
    assert!(!shown(cx, window, "layout-preset", 8));
    assert!(shown(cx, window, "layout-digit", 6));
    assert!(shown(cx, window, "layout-digit", 7));
    press(cx, window, &view, "6");
    assert!(view.read_with(cx, |view, _| view.tiles.switcher.is_none()));
    assert_eq!(current_tiles(cx, &view), Tiles::parse(REVIEW).unwrap());
}

#[gpui_kit::test]
fn a_tenth_preset_shows_without_a_digit_and_arrows_reach_it(cx: &mut TestAppContext) {
    let presets = [
        ("A", "row(20, pane, pane)"),
        ("B", "row(30, pane, pane)"),
        ("C", "row(40, pane, pane)"),
        ("D", "row(60, pane, pane)"),
        ("E", "row(80, pane, pane)"),
    ];
    let (window, view) = with_user_presets(cx, &presets);
    press(cx, window, &view, "ctrl-l");
    assert!(shown(cx, window, "layout-preset", 10));
    assert!(shown(cx, window, "layout-digit", 9));
    assert!(!shown(cx, window, "layout-digit", 10));
    for _ in 0..12 {
        press(cx, window, &view, "right");
    }
    press(cx, window, &view, "enter");
    assert_eq!(
        current_tiles(cx, &view),
        Tiles::parse("row(80, pane, pane)").unwrap()
    );
}

#[gpui_kit::test]
fn a_preset_with_a_broken_tile_string_is_skipped(cx: &mut TestAppContext) {
    let (window, view) =
        with_user_presets(cx, &[("Broken", "row(95, pane, pane)"), ("Logs", LOGS)]);
    press(cx, window, &view, "ctrl-l");
    assert!(shown(cx, window, "layout-preset", 6));
    assert!(!shown(cx, window, "layout-preset", 7));
    press(cx, window, &view, "6");
    assert_eq!(current_tiles(cx, &view), Tiles::parse(LOGS).unwrap());
}

#[gpui_kit::test]
fn reloaded_presets_update_the_open_switcher(cx: &mut TestAppContext) {
    let (window, view) = with_user_presets(cx, &[("Review", REVIEW)]);
    press(cx, window, &view, "ctrl-l");
    assert!(!shown(cx, window, "layout-preset", 7));
    let values = view.read_with(cx, |view, _| {
        view.values
            .clone()
            .with_layout_presets([("Review", REVIEW), ("Logs", LOGS)])
    });
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            let news = SettingsNews::Applied {
                values,
                generation: 1,
            };
            view.take_settings(news, window, cx);
        });
    })
    .expect("window");
    assert!(shown(cx, window, "layout-preset", 7));
    press(cx, window, &view, "7");
    assert_eq!(current_tiles(cx, &view), Tiles::parse(LOGS).unwrap());
}

#[gpui_kit::test]
fn the_current_layout_equal_to_a_user_preset_is_pressed(cx: &mut TestAppContext) {
    let (window, view) = with_user_presets(cx, &[("Review", REVIEW), ("Logs", LOGS)]);
    press(cx, window, &view, "ctrl-l");
    press(cx, window, &view, "7");
    press(cx, window, &view, "ctrl-l");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find(("layout-preset", 7usize)).checked(), Some(true));
        assert_eq!(
            window.find(("layout-preset", 6usize)).checked(),
            Some(false)
        );
        assert_eq!(
            window.find(("layout-preset", 1usize)).checked(),
            Some(false)
        );
    })
    .expect("window");
}
