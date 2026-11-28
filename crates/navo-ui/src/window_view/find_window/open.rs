//! Opening, placing, focusing and closing the floating find-results window.

use gpui_kit::{DisplayId, WindowKind};

use super::super::placement::{self, Shown};
use super::super::{NavoWindow, raw_handle};
use super::FindWindow;
use crate::prelude::*;
use crate::window_view::hand_off::RawWindowHandle;

/// Smallest logical width of the results window (FR-D2).
const MIN_W: f32 = 420.0;
/// Smallest logical height of the results window (FR-D2).
const MIN_H: f32 = 300.0;

#[cfg(test)]
pub(in crate::window_view) fn test_raw_handle(handle: AnyWindowHandle) -> RawWindowHandle {
    // Slotmap keys pack a version into the high bits; the low 32 bits identify the window.
    let id = handle.window_id().as_u64() as u32;
    RawWindowHandle::Web(raw_window_handle::WebWindowHandle::new(id))
}

impl NavoWindow {
    /// Opens the floating find-results window, or activates the existing one if already open.
    pub(in crate::window_view) fn open_find(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.activate_open_find(cx) {
            return;
        }
        let (display_id, bounds) = self.find_placement(window, cx);
        let options = WindowOptions {
            window_bounds: Some(bounds),
            window_min_size: Some(size(px(MIN_W), px(MIN_H))),
            titlebar: None,
            kind: WindowKind::Floating,
            is_movable: true,
            is_resizable: true,
            is_minimizable: false,
            display_id,
            app_id: Some(navo_core::brand::SLUG.into()),
            ..WindowOptions::default()
        };
        let book = self.find.book().clone();
        let navo = cx.weak_entity();
        let navo_handle = window.window_handle();
        let mut find_raw = None;
        let opened = crate::open_window(options, cx, |find_window, cx| {
            find_raw = native_raw(find_window);
            cx.new(|cx| FindWindow::new(navo, navo_handle, book, cx))
        });
        let (handle, view) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                tracing::warn!(%error, "the find results window did not open");
                return;
            }
        };
        self.find.set_window(handle, view);
        self.find.set_raw(find_raw);
        self.check_find_owner(find_raw, window);
        cx.notify();
    }

    /// Saves the find-results window placement and closes the floating window.
    pub(in crate::window_view) fn close_find(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.find.window_handle() else {
            return;
        };
        let shown = cx.update_window(handle, |_, window, cx| {
            let shown = Shown {
                bounds: window.window_bounds(),
                display: window.display(cx).map(|display| display.id()),
            };
            window.remove_window();
            shown
        });
        if let Ok(shown) = shown {
            let screens = placement::screens(cx);
            if let Some((setup, saved)) = placement::placement(&screens, shown) {
                self.browser.find_memory_mut().remember(&setup, saved);
            }
        }
        self.find.clear_window();
    }

    /// Activates an already-open results window; clears stale state when it is gone.
    fn activate_open_find(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(handle) = self.find.window_handle() else {
            return false;
        };
        let active = cx
            .update_window(handle, |_, window, _| window.activate_window())
            .is_ok();
        if active {
            self.find.set_hidden(false);
        } else {
            self.find.clear_window();
        }
        active
    }

    /// Restores the pin and checks the native owner right after the window opened.
    fn check_find_owner(&self, find_raw: Option<RawWindowHandle>, window: &Window) {
        let (Some(find_raw), Some(navo_raw)) = (find_raw, native_raw(window)) else {
            return;
        };
        if self.browser.state().find_memory().pinned {
            (self.find.hooks().pin)(find_raw, true);
        }
        if (self.find.hooks().ensure_owner)(find_raw, navo_raw) {
            tracing::warn!("the find results window owner had to be repaired after open");
        }
    }

    /// Saved placement when one applies, else the first placement beside the focused pane.
    fn find_placement(&self, window: &Window, cx: &App) -> (Option<DisplayId>, WindowBounds) {
        let screens = placement::screens(cx);
        let saved = self.browser.state().find_memory().placement();
        if !saved.is_empty()
            && let Some((display, bounds)) = placement::opening(&screens, saved)
        {
            return (Some(display), bounds);
        }
        let display = window.display(cx).map(|display| display.id());
        let bounds = self.first_find_bounds(window);
        let bounds = screens
            .iter()
            .find(|screen| Some(screen.id) == display)
            .map_or(bounds, |screen| {
                placement::keep_on(bounds, screen.work_area)
            });
        (display, bounds)
    }

    /// First placement: over the half of Navo's window not holding the focused pane,
    /// 45% of Navo's width (min 420 px) and 80% of its height (min 300 px).
    fn first_find_bounds(&self, window: &Window) -> WindowBounds {
        let navo = window.bounds();
        let width = (navo.size.width * 0.45).max(px(MIN_W));
        let height = (navo.size.height * 0.8).max(px(MIN_H));
        let half = navo.size.width * 0.5;
        let inset = (half - width) * 0.5;
        let x = if self.focused_pane_on_left() {
            navo.origin.x + half + inset
        } else {
            navo.origin.x + inset
        };
        let y = navo.origin.y + (navo.size.height - height) * 0.5;
        WindowBounds::Windowed(Bounds {
            origin: point(x, y),
            size: size(width, height),
        })
    }

    /// Whether the focused pane's centre sits in the left half of the layout.
    fn focused_pane_on_left(&self) -> bool {
        let state = self.browser.state();
        let Some(window) = state.window(state.focused_window()) else {
            return true;
        };
        let layout = window.layout();
        let focused = layout.focused();
        layout
            .rects()
            .into_iter()
            .find(|(pane, _)| *pane == focused)
            .is_none_or(|(_, rect)| rect.x + rect.width * 0.5 <= 0.5)
    }
}

/// The window's native raw handle; test builds fall back to a synthetic handle per window.
fn native_raw(window: &Window) -> Option<RawWindowHandle> {
    let raw = raw_handle(window);
    #[cfg(test)]
    let raw = raw.or_else(|| Some(test_raw_handle(window.window_handle())));
    raw
}

// Pins host lifecycle entry points until key and close atoms wire them.
const _: fn(&mut NavoWindow, &mut Window, &mut Context<NavoWindow>) = NavoWindow::open_find;
const _: fn(&mut NavoWindow, &mut Context<NavoWindow>) = NavoWindow::close_find;
