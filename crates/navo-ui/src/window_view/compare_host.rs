//! The window side of the compare screen: every call between `NavoWindow` and the screen goes
//! through here (NC54, NC55).

use super::FileHooks;
use super::NavoWindow;
use super::compare_screen::items_of;
use crate::browser::FileDiffRequest;
use crate::compare_diff::{CompareOptions, PairRequest};
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

    /// Drains the Stream's and the file diff's inboxes; true when the screen changed. Updates of
    /// a closed or replaced file screen are dropped.
    pub(in crate::window_view) fn host_poll(&mut self, cx: &mut Context<Self>) -> bool {
        self.poll_compare_screen(cx)
    }

    /// Gives focus back to the open file diff; false when none is open.
    pub(in crate::window_view) fn host_focus_file(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.focus_file_screen(window, cx)
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
}
