//! Redraw requests made while the window draws, for the width probes of the topbar and Folders.

use gpui_kit::Context;

/// Notifies the view after the current frame ends. GPUI drops the window-dirty part of a
/// `cx.notify()` made inside a prepaint or paint callback, so the frame waits for input (B7, B3).
pub(crate) fn notify_after_frame<T: 'static>(cx: &mut Context<T>) {
    let view = cx.entity_id();
    cx.defer(move |cx| cx.notify(view));
}
