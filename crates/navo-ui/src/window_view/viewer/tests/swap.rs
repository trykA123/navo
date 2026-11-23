//! A large text file shows the paged Text view on its first frame; the editor is read on a
//! background task and swapped in afterwards, keeping the scroll and a pending Ctrl+E (AU12).

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::{Setup, editor_of, has_editor, kind, press_only, redraw, text_of, top, window_on};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{fake, press, settle};
use crate::window_view::viewer::PagedKind;

const LARGE: usize = 15 * 1024 * 1024;
/// More frames than the swap needs: one to read, then layout, pin and show.
const SWAP_FRAMES: usize = 8;

/// Numbered short lines, `LARGE` bytes or a little more.
fn large_text() -> Vec<u8> {
    let mut text = Vec::with_capacity(LARGE + 32);
    let mut line = 0_u64;
    while text.len() < LARGE {
        text.extend_from_slice(format!("line {line}\n").as_bytes());
        line += 1;
    }
    text
}

/// Opens the large file with F3 and draws the first frame after the open finishes.
fn open_large(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<NavoWindow>, Vec<u8>) {
    let bytes = large_text();
    let (window, view, _) = window_on(cx, fake(), Setup::new(bytes.clone()));
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    settle(cx, &view);
    redraw(cx, window);
    (window, view, bytes)
}

/// Delivers the next frame, lets the background read finish and draws the result.
fn next_frame(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.simulate_next_frame(cx))
        .expect("window");
    cx.run_until_parked();
    redraw(cx, window);
}

/// Delivers frames until the editor has replaced the paged view; panics after `SWAP_FRAMES`.
pub(super) fn until_swapped(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) {
    for _ in 0..SWAP_FRAMES {
        if has_editor(cx, view) {
            return;
        }
        next_frame(cx, window);
    }
    assert!(has_editor(cx, view), "the editor is swapped in");
}

#[gpui_kit::test]
fn a_large_text_file_draws_its_first_frame_before_the_editor_is_built(cx: &mut TestAppContext) {
    let (window, view, bytes) = open_large(cx);
    assert_eq!(
        kind(cx, &view),
        Some(PagedKind::Text),
        "the first frame shows the paged Text view"
    );
    assert!(!has_editor(cx, &view), "the editor is not built yet");
    until_swapped(cx, window, &view);
    let editor = editor_of(cx, &view).expect("the editor is swapped in");
    let len = editor.read_with(cx, |state, _| state.text().len());
    assert_eq!(len, bytes.len());
}

#[gpui_kit::test]
fn the_swap_keeps_the_top_line_of_the_paged_view(cx: &mut TestAppContext) {
    let (window, view, bytes) = open_large(cx);
    press(cx, window, &view, "pagedown");
    press(cx, window, &view, "pagedown");
    let at = usize::try_from(top(cx, &view)).expect("offset");
    assert!(at > 0, "the paged view scrolled");
    let row = bytes[..at].iter().filter(|byte| **byte == b'\n').count();
    for _ in 0..SWAP_FRAMES {
        next_frame(cx, window);
        let Some(editor) = editor_of(cx, &view) else {
            continue;
        };
        let rows = editor.read_with(cx, |state, _| state.visible_row_range());
        assert_eq!(
            rows.map(|rows| rows.start),
            Some(row),
            "the first frame that shows the editor has the paged view's top line"
        );
        return;
    }
    panic!("the editor is not swapped in after {SWAP_FRAMES} frames");
}

#[gpui_kit::test]
fn ctrl_e_before_the_swap_starts_editing_after_it(cx: &mut TestAppContext) {
    let (window, view, _) = open_large(cx);
    assert!(!has_editor(cx, &view), "Ctrl+E reaches the paged view");
    press(cx, window, &view, "ctrl-e");
    assert_eq!(text_of(cx, window, "viewer-message"), "");
    until_swapped(cx, window, &view);
    let editing = view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref().and_then(|viewer| viewer.body.as_ref());
        body.is_some_and(|body| body.is_editing(cx))
    });
    assert!(
        editing,
        "the Ctrl+E typed before the swap is applied after it"
    );
}
