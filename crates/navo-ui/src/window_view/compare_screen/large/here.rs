//! "Show bytes here" on the large body (V41d, V41e): the file byte offset of a line over 1 MiB,
//! going back to a line when Show text returns from bytes, and the long-line tests the menu and
//! the command need because a Large pair has no text view.

use std::ops::Range;

use super::{Anchor, LargeShown, LineCache};
use crate::compare_diff::{LongHit, is_long};

impl LargeShown {
    /// The file byte offset where `anchor`'s line starts, when that line is cached.
    pub(in crate::window_view::compare_screen) fn start_of(&self, anchor: Anchor) -> Option<u64> {
        let (left, line) = match anchor {
            Anchor::Left(line) => (true, line),
            Anchor::Right(line) => (false, line),
        };
        self.lines.borrow().start_of(left, line)
    }

    /// `hit` when one of its cached lines is over 1 MiB; its numbers are line indices here.
    pub(in crate::window_view::compare_screen) fn long_hit(&self, hit: LongHit) -> Option<LongHit> {
        let cache = self.lines.borrow();
        let long = |left: bool, line: Option<u32>| {
            line.is_some_and(|line| cache.line(left, u64::from(line)).is_some_and(is_long))
        };
        (long(true, hit.left) || long(false, hit.right)).then_some(hit)
    }

    /// The first line over 1 MiB in view as a menu aim: the left side's first, else the
    /// right's; `None` when the cached lines in view hold none.
    pub(in crate::window_view::compare_screen) fn long_in_view(&self) -> Option<LongHit> {
        let top = self.top();
        let rows = top..top.saturating_add(self.visible()).min(self.map.count());
        let cache = self.lines.borrow();
        let left = self.first_long(rows.clone(), true, &cache);
        let right = self.first_long(rows, false, &cache);
        match (left, right) {
            (Some(line), _) => Some(LongHit {
                left: Some(line),
                right: None,
            }),
            (None, Some(line)) => Some(LongHit {
                left: None,
                right: Some(line),
            }),
            (None, None) => None,
        }
    }

    /// The number of the first line over 1 MiB on the left (`left`) or right side of list rows
    /// `rows`, as a cached line; `None` when no row holds one.
    fn first_long(&self, mut rows: Range<u64>, left: bool, cache: &LineCache) -> Option<u32> {
        rows.find_map(|row| {
            let shown = self.map.at(row);
            let line = if left { shown.left } else { shown.right };
            line.filter(|at| cache.line(left, *at).is_some_and(is_long))
                .and_then(|at| u32::try_from(at).ok())
        })
    }

    /// Whether the pair has any change to walk to.
    pub(in crate::window_view::compare_screen) fn has_changes(&self) -> bool {
        !self.map.hunks().is_empty()
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
