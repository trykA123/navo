//! Where the hex body starts and what it shows on top, for the Alt+V Text / Bytes switch
//! (V41b): a byte offset of the left file becomes the top row, and the top row an offset.
//! A rebuild (regions landing) keeps the same left byte on top, so a seek made before the
//! content rows exist still lands on its byte.

use super::HexView;

/// The byte kept on top across a rebuild: a left offset, or the right one on a gap row.
#[derive(Debug, Clone, Copy)]
pub(in crate::window_view::compare_screen) enum Anchor {
    Left(u64),
    Right(u64),
}

impl HexView {
    /// The left byte offset of the first row shown, or the offset still waiting to be shown.
    /// A gap row reports the next row that has a left side.
    pub(in crate::window_view::compare_screen) fn top_offset(&self, visible: usize) -> u64 {
        if let Some(Anchor::Left(start)) = self.start {
            return start;
        }
        let top = self.viewport(visible).start;
        self.pinned().unwrap_or_else(|| self.rows.left_from(top))
    }

    /// Starts the body at `at` (a byte of the left file, or of the right one for a line only it
    /// has) once its rows are laid out.
    pub(in crate::window_view::compare_screen) fn seek(&mut self, at: Anchor) {
        self.start = Some(at);
    }

    /// Puts a pending start on the top row; called once the rows exist. Only a left offset is
    /// pinned: a right one lands on its row, and a rebuild then keeps that row's own byte.
    pub(super) fn apply_start(&mut self) {
        match self.start.take() {
            Some(Anchor::Left(offset)) => {
                self.top = self.rows.row_of_left(offset);
                self.pin = Some(offset);
            }
            Some(Anchor::Right(offset)) => self.top = self.rows.row_of_right(offset),
            None => {}
        }
    }

    /// The offset sought while its row is still the top row.
    fn pinned(&self) -> Option<u64> {
        self.pin
            .filter(|&offset| self.rows.row_of_left(offset) == self.top)
    }

    /// The byte on top now, taken before the rows are swapped; the left one, else the right
    /// one of a gap row (D16: arriving rows never move the top line); none before the rows exist.
    pub(super) fn anchor(&self) -> Option<Anchor> {
        if !self.installed() || self.rows.count() == 0 {
            return None;
        }
        if let Some(offset) = self.pinned() {
            return Some(Anchor::Left(offset));
        }
        let row = self.rows.row(self.top)?;
        Some(match (row.left, row.right) {
            (Some(left), _) => Anchor::Left(left.at),
            (None, Some(right)) => Anchor::Right(right.at),
            (None, None) => Anchor::Left(self.rows.left_from(self.top)),
        })
    }

    /// Puts the row that holds `anchor` back on top after the rows were swapped.
    pub(super) fn reanchor(&mut self, anchor: Option<Anchor>) {
        match anchor {
            Some(Anchor::Left(offset)) => self.top = self.rows.row_of_left(offset),
            Some(Anchor::Right(offset)) => self.top = self.rows.row_of_right(offset),
            None => {}
        }
    }

    /// Whether the body has its rows and the bytes of the visible ones (or a failure to show),
    /// so a view kept on screen for the switch can go.
    pub(in crate::window_view::compare_screen) fn ready(&self, visible: usize) -> bool {
        self.installed() && !self.unfetched(&self.viewport(visible))
    }
}
