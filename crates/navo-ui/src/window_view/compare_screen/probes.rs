//! Test probes of the compare screen: the open file screen, its look, drawers and map, and an
//! empty Stream under the file diff.

use std::sync::Arc;

use gpui_kit::FocusHandle;

use super::hex::HexProbe;
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

    /// The open file diff's look, to change it in tests.
    #[cfg(test)]
    pub(in crate::window_view) fn file_look_mut(&mut self) -> Option<&mut look::Look> {
        Some(&mut self.compare.screen.as_mut()?.file.as_mut()?.look)
    }

    /// The sideways offset, cut and extent of the open text view.
    #[cfg(test)]
    pub(in crate::window_view) fn file_sideways(&self) -> Option<super::SidewaysSeen> {
        let file = self.compare.screen.as_ref()?.file.as_ref()?;
        Some(file.view.as_ref()?.sideways_seen())
    }

    /// The open text view's wrap index and width (columns), its top list item and the items
    /// that fit in the list, at rows `row_height` high.
    #[cfg(test)]
    pub(in crate::window_view) fn file_wrap(
        &self,
        row_height: Pixels,
    ) -> Option<(
        Option<crate::compare_diff::WrapIndex>,
        Option<u32>,
        usize,
        usize,
    )> {
        let file = self.compare.screen.as_ref()?.file.as_ref()?;
        Some(file.view.as_ref()?.wrap_seen(row_height))
    }

    /// Marks change `at` as the one the open file diff centres when its text lands, the way
    /// Enter on a Stream card does. False when no file diff is open.
    #[cfg(test)]
    pub(in crate::window_view) fn file_open_at(&mut self, at: usize) -> bool {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return false;
        };
        file.at = Some(at);
        true
    }

    /// How far down from the top of the list the line the file diff's menu anchors on ends, in px:
    /// the picked line, else the current change's first line, else the first line. `None` when no
    /// text pair is open.
    #[cfg(test)]
    pub(in crate::window_view) fn file_line_bottom(
        &mut self,
        layout: crate::compare_diff::Layout,
        row_height: Pixels,
    ) -> Option<Pixels> {
        let file = self.compare.screen.as_mut()?.file.as_mut()?;
        let text = Arc::clone(file.view.as_ref()?.text());
        let spot = file.pick.cursor_spot(&text);
        Some(file.view.as_mut()?.line_bottom(layout, spot, row_height))
    }

    /// The cursor of the list of changes.
    pub(in crate::window_view) fn file_drawer_cursor(&self) -> Option<usize> {
        Some(self.compare.screen.as_ref()?.file.as_ref()?.drawers.cursor)
    }

    /// The open hex body: current region, built rows, row count, alignment and gap rows.
    #[cfg(test)]
    pub(in crate::window_view) fn file_hex_probe(&mut self) -> Option<HexProbe> {
        let hex = self.compare.screen.as_mut()?.file.as_mut()?.hex.as_mut()?;
        Some(hex.probe(24))
    }

    /// Puts the line holding left byte `offset` on top of the text list at the next draw.
    #[cfg(test)]
    pub(in crate::window_view) fn file_seek_offset(&mut self, offset: u64) {
        let view = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        if let Some(view) = view.and_then(|file| file.view.as_mut()) {
            view.seek_offset(offset);
        }
    }

    /// How many rows the hex body shows in `window`.
    #[cfg(test)]
    pub(in crate::window_view) fn file_hex_visible(&self, window: &Window, cx: &App) -> usize {
        super::hex::visible_rows(crate::tokens::current(cx), window)
    }

    /// Drops the fetched hex rows, for the unfetched-row probe.
    #[cfg(test)]
    pub(in crate::window_view) fn file_hex_forget_rows(&mut self) {
        if let Some(hex) = self
            .compare
            .screen
            .as_mut()
            .and_then(|screen| screen.file.as_mut())
            .and_then(|file| file.hex.as_mut())
        {
            hex.forget_rows();
        }
    }

    /// Switches the hex alignment and issues its run, without letting background work land.
    #[cfg(test)]
    pub(in crate::window_view) fn file_hex_set_align(
        &mut self,
        align: crate::compare_diff::HexAlign,
    ) {
        if let Some(hex) = self
            .compare
            .screen
            .as_mut()
            .and_then(|screen| screen.file.as_mut())
            .and_then(|file| file.hex.as_mut())
        {
            hex.set_align(align);
            hex.request_regions();
        }
    }

    /// Moves the hex scrollbar thumb, as a drag would.
    #[cfg(test)]
    pub(in crate::window_view) fn file_hex_set_drag(&mut self, fraction: f64) {
        if let Some(hex) = self
            .compare
            .screen
            .as_mut()
            .and_then(|screen| screen.file.as_mut())
            .and_then(|file| file.hex.as_mut())
        {
            hex.set_scroll_fraction(fraction);
        }
    }

    /// Feeds fractional wheel rows for a body `visible` rows tall; returns the top row.
    #[cfg(test)]
    pub(in crate::window_view) fn file_hex_wheel(
        &mut self,
        rows: f64,
        visible: usize,
    ) -> Option<usize> {
        let hex = self.compare.screen.as_mut()?.file.as_mut()?.hex.as_mut()?;
        hex.wheel_rows(rows, visible);
        Some(hex.top())
    }

    /// The labels of the View menu entries that switch the open pair between Text and Bytes.
    pub(in crate::window_view) fn file_switch_labels(&self) -> Vec<String> {
        let Some(file) = self.compare.screen.as_ref().and_then(|s| s.file.as_ref()) else {
            return Vec::new();
        };
        super::switch::entries(file)
            .into_iter()
            .filter_map(|entry| match entry {
                crate::window_view::viewer::MenuEntry::Run(label, ..) => Some(label.to_string()),
                _ => None,
            })
            .collect()
    }

    /// The labels of the file diff's menu entries for the line the menu opened on.
    pub(in crate::window_view) fn file_menu_labels(&self) -> Vec<String> {
        let file = self.compare.screen.as_ref().and_then(|s| s.file.as_ref());
        file.map(super::FileScreen::menu_labels).unwrap_or_default()
    }
}
