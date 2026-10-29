use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, px};

use super::{fake, home_beside_dest, open_unsettled, options_for, settle};
use crate::window_view::FerriteWindow;

fn open_before_listing(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    open_unsettled(cx, options_for(fs, state), px(1000.))
}

fn press_now(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| window.press(key, cx))
        .expect("window");
}

fn has_listing(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> bool {
    view.read_with(cx, |view, _| {
        view.browser.state().active_tab().listing().is_some()
    })
}

fn cursor_name(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<String> {
    view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        tab.listing()
            .and_then(|l| l.get(tab.view().cursor))
            .map(|e| e.name.clone())
    })
}

fn filter_text(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<String> {
    view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        view.browser.filter_text(tab).map(str::to_owned)
    })
}

#[gpui_kit::test]
fn down_pressed_before_the_first_listing_moves_the_cursor_once_it_lands(cx: &mut TestAppContext) {
    let (window, view) = open_before_listing(cx);
    assert!(!has_listing(cx, &view));
    press_now(cx, window, "down");
    settle(cx, &view);
    assert_eq!(cursor_name(cx, &view).as_deref(), Some("a.txt"));
}

#[gpui_kit::test]
fn a_letter_typed_before_the_first_listing_filters_once_it_lands(cx: &mut TestAppContext) {
    let (window, view) = open_before_listing(cx);
    press_now(cx, window, "d");
    settle(cx, &view);
    assert_eq!(filter_text(cx, &view).as_deref(), Some("d"));
}

#[gpui_kit::test]
fn keys_typed_before_the_first_listing_replay_in_order(cx: &mut TestAppContext) {
    let (window, view) = open_before_listing(cx);
    press_now(cx, window, "d");
    press_now(cx, window, "o");
    settle(cx, &view);
    assert_eq!(filter_text(cx, &view).as_deref(), Some("do"));
}

#[gpui_kit::test]
fn enter_pressed_before_the_first_listing_opens_the_cursor_folder(cx: &mut TestAppContext) {
    let (window, view) = open_before_listing(cx);
    press_now(cx, window, "enter");
    settle(cx, &view);
    let folder = view.read_with(cx, |view, _| {
        view.browser
            .state()
            .active_tab()
            .location()
            .as_str()
            .to_owned()
    });
    assert!(folder.ends_with("docs"), "{folder}");
}

#[gpui_kit::test]
fn keys_after_the_hold_time_are_not_held(cx: &mut TestAppContext) {
    let (window, view) = open_before_listing(cx);
    view.update(cx, |view, _| {
        view.first_keys.until = Some(Instant::now() - Duration::from_millis(1));
    });
    press_now(cx, window, "down");
    settle(cx, &view);
    assert_eq!(cursor_name(cx, &view).as_deref(), Some("docs"));
}
