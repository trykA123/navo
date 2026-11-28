//! Opening, placing, focusing and closing the floating find-results window.

use gpui_kit::{DisplayId, WeakEntity, WindowKind};

use super::super::placement::{self, Shown};
use super::super::{NavoWindow, raw_handle};
use super::FindWindow;
use crate::find_results::book::TRIM_LINES;
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
    /// Focuses or opens the results window when tabs exist; otherwise opens Ctrl+K Content mode.
    pub(in crate::window_view) fn focus_find_results(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.find.book().read(cx).is_empty() {
            self.open_content(window, cx);
            return;
        }
        self.open_find(window, cx);
    }

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
        let pinned = self.browser.state().find_memory().pinned;
        let navo = cx.weak_entity();
        let navo_handle = window.window_handle();
        let mut find_raw = None;
        let opened = crate::open_window(options, cx, |find_window, cx| {
            find_raw = native_raw(find_window);
            wire_find_close(find_window, navo.clone(), navo_handle, cx);
            let view = cx.new(|cx| FindWindow::new(navo, navo_handle, book, pinned, cx));
            find_window.activate_window();
            view.update(cx, |view, cx| view.focus(find_window, cx));
            view
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

    /// Saves the find book and placement, then closes the floating window.
    pub(in crate::window_view) fn close_find(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self.find.window_handle() else {
            self.finish_find_close(None, cx);
            return;
        };
        let shown = cx.update_window(handle, |_, window, cx| {
            let shown = find_shown(window, cx);
            window.remove_window();
            shown
        });
        self.finish_find_close(shown.ok(), cx);
    }

    /// The user closed the results window: activate Navo first so focus never drops behind
    /// another app, then persist the book, trim stored hits and let the platform close it.
    pub(in crate::window_view) fn close_find_by_user(
        &mut self,
        navo: AnyWindowHandle,
        find: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = cx.update_window(navo, |_, window, _| window.activate_window());
        let shown = find_shown(find, cx);
        self.finish_find_close(Some(shown), cx);
        self.find
            .book()
            .update(cx, |book, _| book.trim_all(TRIM_LINES));
    }

    /// Minimise: hide the results window through the platform hook and hand focus to Navo.
    pub(in crate::window_view) fn hide_find(
        &mut self,
        navo: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if !self.find.is_open() || self.find.is_hidden() {
            return;
        }
        if let Some(hook) = &self.window_shown {
            hook(self.find.raw(), false);
        }
        self.find.set_hidden(true);
        let _ = cx.update_window(navo, |_, window, _| window.activate_window());
    }

    /// Toggles the results window's always-on-top pin, saving it and notifying the platform hook.
    pub(in crate::window_view) fn toggle_find_pin(&mut self, cx: &mut Context<Self>) {
        let next = !self.browser.state().find_memory().pinned;
        self.browser.find_memory_mut().pinned = next;
        if let Some(find_raw) = self.find.raw() {
            (self.find.hooks().pin)(find_raw, next);
        }
        if let Some(view) = self.find.view().cloned() {
            view.update(cx, |view, cx| view.set_pinned(next, cx));
        }
        cx.notify();
    }

    /// Persists queries (the pin stays), remembers placement when known, clears window state.
    fn finish_find_close(&mut self, shown: Option<Shown>, cx: &mut Context<Self>) {
        let saved = self.find.book().read(cx).saved();
        let memory = self.browser.find_memory_mut();
        memory.set_queries(saved);
        if let Some(shown) = shown {
            let screens = placement::screens(cx);
            if let Some((setup, place)) = placement::placement(&screens, shown) {
                memory.remember(&setup, place);
            }
        }
        self.find.clear_window();
    }

    /// Activates an already-open results window; clears stale state when it is gone.
    fn activate_open_find(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(handle) = self.find.window_handle() else {
            return false;
        };
        let view = self.find.view().cloned();
        if self.find.is_hidden()
            && let Some(hook) = &self.window_shown
        {
            hook(self.find.raw(), true);
        }
        let active = cx
            .update_window(handle, |_, window, cx| {
                window.activate_window();
                if let Some(view) = &view {
                    view.update(cx, |view, cx| view.focus(window, cx));
                }
            })
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

/// Wires the native close (X) of the results window to the owner's user-close path.
fn wire_find_close(
    find_window: &mut Window,
    navo: WeakEntity<NavoWindow>,
    navo_handle: AnyWindowHandle,
    cx: &mut App,
) {
    find_window.on_window_should_close(cx, move |window, cx| {
        let _ = navo.update(cx, |view, cx| {
            view.close_find_by_user(navo_handle, window, cx)
        });
        true
    });
}

/// The window's current placement snapshot.
fn find_shown(window: &Window, cx: &App) -> Shown {
    Shown {
        bounds: window.window_bounds(),
        display: window.display(cx).map(|display| display.id()),
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
const _: fn(&mut NavoWindow, AnyWindowHandle, &mut Context<NavoWindow>) = NavoWindow::hide_find;
