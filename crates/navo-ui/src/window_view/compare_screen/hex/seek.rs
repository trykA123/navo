//! Where the hex body starts and what it shows on top, for the Alt+V Text / Bytes switch
//! (V41b): a byte offset of the left file becomes the top row, and the top row an offset.
//! A rebuild (regions landing) keeps the same left byte on top, so a seek made before the
//! content rows exist still lands on its byte.

use super::HexView;

impl HexView {
    /// The left byte offset of the first row shown, or the offset still waiting to be shown.
    /// A gap row reports the next row that has a left side.
    pub(in crate::window_view::compare_screen) fn top_offset(&self, visible: usize) -> u64 {
        if let Some(start) = self.start {
            return start;
        }
        let top = self.viewport(visible).start;
        self.pinned().unwrap_or_else(|| self.rows.left_from(top))
    }

    /// Starts the body at left byte `offset` once its rows are laid out.
    pub(in crate::window_view::compare_screen) fn seek(&mut self, offset: u64) {
        self.start = Some(offset);
    }

    /// Puts a pending start on the top row; called once the rows exist.
    pub(super) fn apply_start(&mut self) {
        if let Some(offset) = self.start.take() {
            self.top = self.rows.row_of_left(offset);
            self.pin = Some(offset);
        }
    }

    /// The offset sought while its row is still the top row.
    fn pinned(&self) -> Option<u64> {
        self.pin
            .filter(|&offset| self.rows.row_of_left(offset) == self.top)
    }

    /// The left byte on top now, taken before the rows are swapped; none before the rows exist.
    pub(super) fn anchor(&self) -> Option<u64> {
        if !self.installed() || self.rows.count() == 0 {
            return None;
        }
        Some(
            self.pinned()
                .unwrap_or_else(|| self.rows.left_from(self.top)),
        )
    }

    /// Puts the row that holds `anchor` back on top after the rows were swapped.
    pub(super) fn reanchor(&mut self, anchor: Option<u64>) {
        if let Some(offset) = anchor {
            self.top = self.rows.row_of_left(offset);
        }
    }

    /// Whether the body has its rows and the bytes of the visible ones (or a failure to show),
    /// so a view kept on screen for the switch can go.
    pub(in crate::window_view::compare_screen) fn ready(&self, visible: usize) -> bool {
        self.installed() && !self.unfetched(&self.viewport(visible))
    }
}
