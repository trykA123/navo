//! Test access to the compare screen entity through its owning window.

use gpui_kit::TestAppContext;

use super::NavoWindow;
use crate::prelude::*;
use crate::window_view::ScreenOwner;

/// Reads the compare screen's owner of `view`.
pub(super) fn screen_read<R>(
    view: &Entity<NavoWindow>,
    cx: &mut TestAppContext,
    read: impl FnOnce(&ScreenOwner, &App) -> R,
) -> R {
    let screen = view.read_with(cx, |view, _| view.compare.screen.clone());
    screen.read_with(cx, read)
}

/// Updates the compare screen's owner of `view`.
pub(super) fn screen_update<R>(
    view: &Entity<NavoWindow>,
    cx: &mut TestAppContext,
    update: impl FnOnce(&mut ScreenOwner, &mut Context<ScreenOwner>) -> R,
) -> R {
    let screen = view.read_with(cx, |view, _| view.compare.screen.clone());
    screen.update(cx, update)
}

/// Updates it with the window, for calls that focus or measure.
pub(super) fn screen_update_in<R>(
    view: &Entity<NavoWindow>,
    window: AnyWindowHandle,
    cx: &mut TestAppContext,
    update: impl FnOnce(&mut ScreenOwner, &mut Window, &mut Context<ScreenOwner>) -> R,
) -> R {
    let screen = view.read_with(cx, |view, _| view.compare.screen.clone());
    window
        .update(cx, |_, window, cx| {
            screen.update(cx, |owner, cx| update(owner, window, cx))
        })
        .expect("window")
}
