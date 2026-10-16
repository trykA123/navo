use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

use super::find::{line_start, numbered_lines};
use super::search::{editor_view, paged_marking};
use super::{Setup, text_window, top, visible};
use crate::window_view::tests::{press, type_in, viewer_open};

fn row_visible(cx: &mut TestAppContext, window: AnyWindowHandle, row: usize) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find("find-all-list").is_some()
            && window
                .within("find-all-list")
                .try_find(("row", row))
                .is_some_and(|found| found.visible())
    })
    .expect("window")
}

#[gpui_kit::test]
fn alt_enter_lists_every_match_and_enter_jumps_to_the_selected_one(cx: &mut TestAppContext) {
    let bytes = numbered_lines();
    let line_900 = line_start(&bytes, 900);
    let (window, view, _) = text_window(cx, Setup::huge(bytes));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "needle");
    press(cx, window, &view, "alt-enter");
    assert!(visible(cx, window, "find-all"));
    let listed = paged_marking(cx, &view, |marking| {
        marking.counted().map(|counted| counted.starts.len())
    });
    assert_eq!(listed, Some(2));
    assert!(row_visible(cx, window, 0));
    assert!(row_visible(cx, window, 1));
    press(cx, window, &view, "down");
    press(cx, window, &view, "enter");
    assert_eq!(top(cx, &view), line_900);
    assert!(visible(cx, window, "find-all"));
    press(cx, window, &view, "escape");
    assert!(!visible(cx, window, "find-all"));
    assert!(viewer_open(cx, &view));
}

#[gpui_kit::test]
fn find_all_in_the_text_editor_jumps_to_the_line_of_a_match(cx: &mut TestAppContext) {
    let (window, view, _) = text_window(cx, Setup::new(b"one\ntwo\nthree one\n".to_vec()));
    press(cx, window, &view, "ctrl-f");
    type_in(cx, window, &view, "one");
    press(cx, window, &view, "alt-enter");
    assert!(row_visible(cx, window, 1));
    press(cx, window, &view, "down");
    press(cx, window, &view, "enter");
    let editor = editor_view(cx, &view);
    let selected = editor.read_with(cx, |view, cx| view.selected(cx));
    assert_eq!(selected, 14..17);
    press(cx, window, &view, "escape");
    assert!(!visible(cx, window, "find-all"));
}
