//! The window side of the compare screen: every call between `NavoWindow` and the screen goes
//! through here (NC54, NC55).

use super::FileHooks;
use super::NavoWindow;
use super::compare_screen::{ScreenEvent, items_of, neighbour};
use crate::browser::FileDiffRequest;
use crate::compare_diff::{CompareOptions, PairRequest, StreamList};
use crate::context_menu::ContextMenu;
use crate::prelude::*;

/// The type that owns the compare screen: `NavoWindow` until N6, then the `CompareScreen` entity.
pub(in crate::window_view) type ScreenOwner = NavoWindow;

impl ScreenOwner {
    /// Opens the file diff Enter asked for; false when none was asked or the app has no hooks.
    pub(in crate::window_view) fn open_file_diff(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(asked) = self.browser.take_file_diff() else {
            return false;
        };
        let FileDiffRequest {
            id,
            path,
            left,
            right,
        } = asked;
        let request = PairRequest {
            id,
            path,
            left,
            right,
        };
        self.show_pair((request, None), window, cx)
    }

    /// Alt+F or "Stream all differences": opens the Stream over every difference under the
    /// compare folder. False outside a compare or without diff hooks.
    pub(in crate::window_view) fn open_compare_stream(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(items) = self.browser.compare_mode().map(items_of) else {
            return false;
        };
        self.open_stream(items, window, cx)
    }

    /// The compare screen drawn in place of the panes: the file diff, else the Stream, else `None`
    /// when no screen is open.
    pub(in crate::window_view) fn host_screen_view(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.compare_screen_view(window, cx)
    }

    /// The open View menu of the file diff or the Stream for the window to draw; `None` while
    /// neither shows one.
    pub(in crate::window_view) fn host_menu_view(&self) -> Option<Entity<ContextMenu>> {
        self.compare_menu_view()
    }

    /// Re-lays the large diff body after a window resize; does nothing with no large body open.
    pub(in crate::window_view) fn host_follow_window(&mut self, window: &Window) {
        self.large_follow_window(window);
    }

    /// Resyncs the Stream's items from the folder compare, then drains the Stream's and the
    /// file diff's inboxes; true when the screen changed. Updates of a closed or replaced file
    /// screen are dropped.
    pub(in crate::window_view) fn host_poll(&mut self, cx: &mut Context<Self>) -> bool {
        let resynced = self.host_resync(cx);
        self.poll_compare_screen(resynced, cx)
    }

    /// Gives focus back to the open file diff; false when none is open.
    pub(in crate::window_view) fn host_focus_file(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.focus_file_screen(window, cx)
    }

    /// Handles one screen event: `Closed` reveals its path and gives focus back to the compare
    /// pane, `OpenSettings` opens Settings at its field, `WalkFile` opens the folder compare's
    /// next or previous file, `Resync` rebuilds the Stream's items. `window` is `None` only for
    /// `Resync` (N4f2).
    pub(in crate::window_view) fn on_screen_event(
        &mut self,
        event: &ScreenEvent,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        match event {
            ScreenEvent::Closed { reveal } => {
                if let Some(path) = reveal {
                    self.browser.compare_reveal(path);
                }
                if let Some(window) = window {
                    self.focus_core_pane(window, cx);
                }
                cx.notify();
            }
            ScreenEvent::OpenSettings(field) => {
                if let Some(window) = window {
                    self.open_settings_at(*field, window, cx);
                }
            }
            ScreenEvent::WalkFile { from, step } => {
                let Some(window) = window else {
                    return;
                };
                let found = self
                    .browser
                    .compare_mode()
                    .and_then(|mode| neighbour(mode, *from, *step));
                if let Some(request) = found {
                    self.show_pair((request, Some(0)), window, cx);
                }
            }
            ScreenEvent::Resync => {
                if self.host_resync(cx) {
                    cx.notify();
                }
            }
        }
    }

    // Owner inputs: N6 gives these the same names on the entity.

    /// The compare settings the screen opens a pair with, read from the owner's values.
    pub(in crate::window_view) fn screen_options(&self) -> CompareOptions {
        CompareOptions::from_values(&self.values)
    }

    /// The app's file hooks, how the screen reads a side's bytes and bodies.
    pub(in crate::window_view) fn screen_files(&self) -> FileHooks {
        self.files.clone()
    }

    /// The compare roots' names as "left ↔ right", or empty outside a compare.
    pub(in crate::window_view) fn screen_sides(&self) -> SharedString {
        let Some(mode) = self.browser.compare_mode() else {
            return SharedString::default();
        };
        let [left, right] = mode.roots().clone().map(|root| {
            root.name()
                .map_or_else(|| root.as_str().to_owned(), |name| name.to_text())
        });
        navo_l10n::t!("compare-screen-sides", left = left, right = right).into()
    }

    /// The compare's rows stamp, its generation and the batches received in it; `None` outside a
    /// compare, and it changes whenever the listed rows do.
    pub(in crate::window_view) fn screen_stamp(&self) -> Option<(u64, u64)> {
        self.browser.compare_mode().map(|mode| mode.rows_stamp())
    }

    /// Whether the folder compare is still checking rows; false outside a compare.
    pub(in crate::window_view) fn screen_checking(&self) -> bool {
        self.browser
            .compare_mode()
            .is_some_and(|mode| mode.checking())
    }

    /// The Stream's items under the compare folder and each item's pair; `None` outside a
    /// compare.
    pub(in crate::window_view) fn screen_items(&self) -> Option<(StreamList, Vec<PairRequest>)> {
        self.browser.compare_mode().map(items_of)
    }

    /// Sends one screen event to the window's handler; N6 turns this into `cx.emit(event)`.
    pub(in crate::window_view) fn emit_screen(
        &mut self,
        event: ScreenEvent,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.on_screen_event(&event, window, cx)
    }
}
