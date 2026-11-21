//! Check of the mouse on the Folders rows: a double click opens a file and toggles a folder once.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::FoldersEvent;
use super::tests::{shown, view};

/// Line 0 is the folder `a`; a double click is two toggles today, so it ends closed.
#[gpui_kit::test]
fn double_click_on_a_folder_row_toggles_it_once(cx: &mut TestAppContext) {
    let (window, view, _) = view(cx);
    cx.update_window(window, |_, window, cx| {
        window.double_click(("folder-line", 0usize), cx)
    })
    .expect("window");
    assert_eq!(shown(&view, cx), ["a", "a/x.txt", "a/b", "c", "z.txt"]);
}

/// Line 2 is `z.txt` with `a`, `c` closed; a double click must emit `OpenPair(7)`.
#[gpui_kit::test]
fn double_click_on_a_file_row_opens_its_pair(cx: &mut TestAppContext) {
    let (window, _view, events) = view(cx);
    cx.update_window(window, |_, window, cx| {
        window.double_click(("folder-line", 2usize), cx)
    })
    .expect("window");
    assert_eq!(*events.borrow(), [FoldersEvent::OpenPair(7)]);
}

/// One click on folder `a` only selects it; it stays closed.
#[gpui_kit::test]
fn a_click_selects_without_opening(cx: &mut TestAppContext) {
    let (window, view, _) = view(cx);
    cx.update_window(window, |_, window, cx| {
        window.click(("folder-line", 1usize), cx)
    })
    .expect("window");
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("c")));
    assert_eq!(shown(&view, cx), ["a", "c", "z.txt"]);
}

/// The chevron of line 1 opens `c` and leaves the cursor on line 0.
#[gpui_kit::test]
fn the_chevron_toggles_and_keeps_the_cursor(cx: &mut TestAppContext) {
    let (window, view, _) = view(cx);
    cx.update_window(window, |_, window, cx| {
        window.click(("chevron", 1usize), cx)
    })
    .expect("window");
    cx.update(|cx| assert_eq!(view.read(cx).selected_path().as_deref(), Some("a")));
    assert_eq!(shown(&view, cx).len(), 4, "c opened with one child");
}
