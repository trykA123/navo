//! The window side of the compare screen: every call between `NavoWindow` and the screen goes
//! through here (NC54, NC55).

use super::NavoWindow;
use super::compare_screen::items_of;
use crate::browser::FileDiffRequest;
use crate::compare_diff::PairRequest;
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
}
