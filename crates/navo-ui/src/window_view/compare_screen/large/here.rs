//! "Show bytes here" on the large body (V41d): the file byte offset of a line over 1 MiB, and
//! going back to a line when Show text returns from bytes.

use super::{Anchor, LargeShown};

impl LargeShown {
    /// The file byte offset where `anchor`'s line starts, when that line is cached.
    pub(in crate::window_view::compare_screen) fn start_of(&self, anchor: Anchor) -> Option<u64> {
        let (left, line) = match anchor {
            Anchor::Left(line) => (true, line),
            Anchor::Right(line) => (false, line),
        };
        self.lines.borrow().start_of(left, line)
    }

    /// Puts `anchor`'s line on top as soon as the line counts are known.
    pub(in crate::window_view::compare_screen) fn seek_line(&mut self, anchor: Anchor) {
        self.sought = Some(anchor);
        self.apply_sought();
    }

    /// Moves to a sought line once the line counts are known, then forgets it.
    pub(super) fn apply_sought(&mut self) {
        if self.map.totals().is_none() {
            return;
        }
        if let Some(anchor) = self.sought.take() {
            self.place(self.map.row_of(anchor), 0.);
        }
    }
}
