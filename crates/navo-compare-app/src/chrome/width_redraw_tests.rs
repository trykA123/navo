//! B3 check: a width probe that changes the topbar layout inside a frame must still ask for a
//! redraw, or the window keeps the stale layout until the next input event.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, px, size};

use super::tests::topbar;

/// Counts the notifications `view` emits from now on.
pub(crate) fn count_notifies<T: 'static>(
    view: &Entity<T>,
    cx: &mut TestAppContext,
) -> Rc<Cell<usize>> {
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    cx.update(|cx| {
        cx.observe(view, move |_, _| seen.set(seen.get() + 1))
            .detach()
    });
    count
}

/// Resizes `window` to `width` px, draws exactly one frame, then lets queued effects run with
/// no input, as the platform would before it goes idle.
pub(crate) fn one_frame_at(window: AnyWindowHandle, width: f32, cx: &mut TestAppContext) {
    cx.simulate_window_resize(window, size(px(width), px(600.)));
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    cx.run_until_parked();
}

/// B3: the topbar compact mode measured during a frame must notify the bar after the frame.
#[gpui_kit::test]
fn topbar_compact_change_in_a_frame_asks_for_a_redraw(cx: &mut TestAppContext) {
    let (window, view, _) = topbar(cx, 780.);
    let notes = count_notifies(&view, cx);
    one_frame_at(window, 500., cx);
    assert!(notes.get() > 0, "the compact topbar was never redrawn");
}
