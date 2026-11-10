//! Test probes of the compare screen: the open file screen, its look, drawers and map, and an
//! empty Stream under the file diff.

use gpui_kit::FocusHandle;

use super::{FileState, look};
use crate::compare_diff::PairRequest;
use crate::prelude::*;
use crate::window_view::NavoWindow;

impl NavoWindow {
    /// The open file screen's request and state, for tests.
    pub(in crate::window_view) fn file_screen(&self) -> Option<(&PairRequest, &FileState)> {
        let file = self.compare.screen.as_ref()?.file.as_ref()?;
        Some((&file.request, &file.state))
    }

    /// The hunks of the large snapshot the open file screen applied last.
    pub(in crate::window_view) fn file_large_hunks(
        &self,
    ) -> Option<&[crate::compare_diff::LargeHunk]> {
        let file = self.compare.screen.as_ref()?.file.as_ref()?;
        file.large.as_ref().map(|large| large.hunks.as_slice())
    }

    /// Asks the open file screen's worker to reload both sides.
    pub(in crate::window_view) fn refresh_file_screen(&mut self) {
        if let Some(file) = self
            .compare
            .screen
            .as_mut()
            .and_then(|screen| screen.file.as_mut())
        {
            file.refresh();
        }
    }

    /// Whether the open file screen has focus.
    pub(in crate::window_view) fn file_screen_focused(&self, window: &Window) -> bool {
        self.compare
            .screen
            .as_ref()
            .and_then(|screen| screen.file.as_ref())
            .is_some_and(|file| file.focus.is_focused(window))
    }

    /// Puts an empty Stream screen under the file diff; returns its focus handle.
    pub(in crate::window_view) fn put_stream_screen(
        &mut self,
        cx: &mut Context<Self>,
    ) -> FocusHandle {
        let list = crate::compare_diff::StreamList::default();
        self.start_stream((list, Vec::new()), cx)
            .expect("the test window has diff hooks")
    }

    /// The open file view's current change, last built rows and row count in `layout`.
    pub(in crate::window_view) fn file_probe(
        &mut self,
        layout: crate::compare_diff::Layout,
    ) -> Option<(Option<usize>, std::ops::Range<usize>, usize)> {
        let file = self.compare.screen.as_mut()?.file.as_mut()?;
        Some(file.view.as_mut()?.probe(layout))
    }

    /// The change map mounted on the open file diff.
    pub(in crate::window_view) fn file_map(
        &self,
    ) -> Option<Entity<crate::compare_diff::map::ChangeMap>> {
        self.compare
            .screen
            .as_ref()?
            .file
            .as_ref()?
            .view
            .as_ref()?
            .map_entity()
    }
}

impl NavoWindow {
    /// The look and the open drawers (changes, details) of the open file diff.
    pub(in crate::window_view) fn file_look(&self) -> Option<(look::Look, bool, bool)> {
        let file = self.compare.screen.as_ref()?.file.as_ref()?;
        Some((file.look, file.drawers.changes, file.drawers.details))
    }

    /// The cursor of the list of changes.
    pub(in crate::window_view) fn file_drawer_cursor(&self) -> Option<usize> {
        Some(self.compare.screen.as_ref()?.file.as_ref()?.drawers.cursor)
    }
}
