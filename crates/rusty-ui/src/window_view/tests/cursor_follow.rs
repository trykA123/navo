use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, point, px};

use super::{fake, home_beside_dest, open_with, press};
use crate::window_view::FerriteWindow;

fn open_long_list(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    for index in 0..500 {
        fs.add_file(&format!("home/f{index:03}.txt"), 1);
    }
    let state = home_beside_dest(&fs);
    open_with(cx, fs, state, Rc::default())
}

fn view_of(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> (u64, u64, u64) {
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let pane = state.pane(state.focused_pane()).expect("pane");
        let shown = pane.active().view();
        (shown.cursor, shown.scroll, pane.page_rows())
    })
}

fn row_visible(
    cx: &mut TestAppContext,
    (window, view): (AnyWindowHandle, &Entity<FerriteWindow>),
    row: u64,
) -> bool {
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within(("list", pane.get()));
        let row = usize::try_from(row).expect("row");
        list.try_find(("row", row)).is_some_and(|row| row.visible())
    })
    .expect("window")
}

fn assert_cursor_shown(
    cx: &mut TestAppContext,
    (window, view): (AnyWindowHandle, &Entity<FerriteWindow>),
) {
    let (cursor, scroll, page) = view_of(cx, view);
    assert!(page < 100, "page rows {page} counts more than the viewport");
    assert!(
        (scroll..scroll + page).contains(&cursor),
        "cursor {cursor} outside rows {scroll}..{}",
        scroll + page
    );
    assert!(
        row_visible(cx, (window, view), cursor),
        "cursor row {cursor} is off-screen"
    );
}

#[gpui_kit::test]
fn arrow_down_past_the_page_scrolls_the_list(cx: &mut TestAppContext) {
    let (window, view) = open_long_list(cx);
    let (_, _, page) = view_of(cx, &view);
    for _ in 0..page + 10 {
        press(cx, window, &view, "down");
    }
    let (_, scroll, _) = view_of(cx, &view);
    assert!(scroll > 0, "the top row stayed at 0");
    assert!(
        !row_visible(cx, (window, &view), 0),
        "row 0 is still on screen"
    );
    assert_cursor_shown(cx, (window, &view));
}

#[gpui_kit::test]
fn page_down_end_and_home_keep_the_cursor_on_screen(cx: &mut TestAppContext) {
    let (window, view) = open_long_list(cx);
    press(cx, window, &view, "pagedown");
    press(cx, window, &view, "pagedown");
    assert_cursor_shown(cx, (window, &view));
    press(cx, window, &view, "end");
    assert_cursor_shown(cx, (window, &view));
    press(cx, window, &view, "pageup");
    assert_cursor_shown(cx, (window, &view));
    press(cx, window, &view, "home");
    assert_cursor_shown(cx, (window, &view));
    assert!(row_visible(cx, (window, &view), 0));
}

fn top_shown_row(
    cx: &mut TestAppContext,
    (window, view): (AnyWindowHandle, &Entity<FerriteWindow>),
) -> Option<u64> {
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within(("list", pane.get()));
        (0..500usize)
            .find(|row| {
                list.try_find(("row", *row))
                    .is_some_and(|row| row.visible())
            })
            .map(|row| row as u64)
    })
    .expect("window")
}

fn click_scrollbar_track(
    cx: &mut TestAppContext,
    (window, view): (AnyWindowHandle, &Entity<FerriteWindow>),
) {
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.find(("list", pane.get())).bounds();
        let at = point(list.right() - px(4.), list.top() + px(300.));
        window.drag(at, point(at.x, at.y + px(50.)), cx);
    })
    .expect("window");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn arrows_after_a_scrollbar_jump_bring_the_cursor_back_into_view(cx: &mut TestAppContext) {
    let (window, view) = open_long_list(cx);
    click_scrollbar_track(cx, (window, &view));
    assert!(
        top_shown_row(cx, (window, &view)) > Some(100),
        "the scrollbar did not move the list"
    );
    press(cx, window, &view, "down");
    assert_cursor_shown(cx, (window, &view));
    assert_eq!(top_shown_row(cx, (window, &view)), Some(1));
}
