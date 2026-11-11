//! Where the hex body starts and what it shows on top, for the Alt+V Text / Bytes switch
//! (V41b): a byte offset of the left file becomes the top row, and the top row an offset.

use super::HexView;

impl HexView {
    /// The left byte offset of the first row shown, or the offset still waiting to be shown.
    pub(in crate::window_view::compare_screen) fn top_offset(&self, visible: usize) -> u64 {
        if let Some(start) = self.start {
            return start;
        }
        self.rows.left_at(self.viewport(visible).start).unwrap_or(0)
    }

    /// Starts the body at left byte `offset` once its rows are laid out.
    pub(in crate::window_view::compare_screen) fn seek(&mut self, offset: u64) {
        self.start = Some(offset);
    }

    /// Puts a pending start on the top row; called once the rows exist.
    pub(super) fn apply_start(&mut self) {
        if let Some(offset) = self.start.take() {
            self.top = self.rows.row_of_left(offset);
        }
    }
}
