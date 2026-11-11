//! Wrapped rows of the file diff (D39.5–D39.6): the wrap index of the open text, built per layout
//! and column width, and the map between pair rows and list items (one item per visual line).

use std::ops::Range;

use super::FileView;
use crate::compare_diff::body::RowCut;
use crate::compare_diff::{Layout, WrapIndex};
use crate::prelude::*;

/// The wrap state of one open text view.
#[derive(Default)]
pub(super) struct Wrapped {
    /// Whether the look asks for wrapped lines.
    on: bool,
    /// The layout and width (columns) the index was built for; `None` while flat.
    key: Option<(Layout, u32)>,
    /// The index and its width; `None` while flat.
    draw: Option<WrapDraw>,
}

/// What the rows closure draws wrapped rows with: the index and the width of a visual line.
#[derive(Clone)]
pub(super) struct WrapDraw {
    index: WrapIndex,
    /// The columns of one visual line (`W` in D39.5).
    cols: u32,
}

impl WrapDraw {
    /// The pair rows that list `items` show, and each item's pair row and visual line in it.
    pub(super) fn place(&self, items: Range<usize>) -> (Range<usize>, Vec<(usize, u32)>) {
        let (mut row, mut sub) = self.index.at(items.start);
        let first = row;
        let mut placed = Vec::with_capacity(items.len());
        for _ in items {
            placed.push((row, sub));
            sub += 1;
            if sub >= self.row_lines(row) {
                (row, sub) = (row + 1, 0);
            }
        }
        let end = placed.last().map_or(first, |&(row, _)| row + 1);
        (first..end, placed)
    }

    /// The items pair row `row` takes; 0 past the last row.
    fn row_lines(&self, row: usize) -> u32 {
        self.index.lines_of(row)
    }

    /// The cut of visual line `sub`: columns `sub·W..(sub+1)·W`, a continuation after the first.
    pub(super) fn cut(&self, sub: u32) -> RowCut {
        let from = sub.saturating_mul(self.cols);
        RowCut {
            cols: Some(from..from.saturating_add(self.cols)),
            continued: sub > 0,
            ..RowCut::default()
        }
    }
}

impl FileView {
    /// Turns line wrap on or off for the next draw; sideways input is ignored while it is on.
    pub(in crate::window_view::compare_screen) fn set_wrap(&mut self, wrap: bool) {
        self.wrapped.on = wrap;
        self.sideways.set_wrap(wrap);
    }

    /// Rebuilds the wrap index when its key (`layout`, width in columns) changed, and keeps the
    /// top pair row on top (rows `row_height` px high) unless a centre is pending, which moves
    /// the list anyway. Flat while wrap is off or before the widths and the text cell are known.
    pub(super) fn index(&mut self, layout: Layout, row_height: Pixels) {
        let key = match (self.wrapped.on, &self.cols, self.sideways.wrap_cols()) {
            (true, Some(_), Some(cols)) => Some((layout, cols)),
            _ => None,
        };
        if key == self.wrapped.key {
            return;
        }
        let top = self.pair_row(self.scroll.top_row(row_height));
        self.wrapped.draw = key
            .zip(self.cols.as_deref())
            .map(|((layout, wrap), cols)| WrapDraw {
                index: WrapIndex::build(&self.text, layout, cols, wrap),
                cols: wrap,
            });
        self.wrapped.key = key;
        if !self.centre {
            self.scroll.scroll_to_top(self.list_row(top));
        }
    }

    /// The first list item of pair row `row`.
    pub(super) fn list_row(&self, row: usize) -> usize {
        self.wrapped
            .draw
            .as_ref()
            .map_or(row, |wrap| wrap.index.first_of(row))
    }

    /// The list items pair row `row` takes: one while flat, 0 past the last row while wrapping.
    pub(super) fn row_lines(&self, row: usize) -> u32 {
        self.wrapped
            .draw
            .as_ref()
            .map_or(1, |wrap| wrap.index.lines_of(row))
    }

    /// Whether wrap is on but its index is not built yet (the widths or the text cell are still
    /// unknown). List indices are flat then, so a centre waits for the frame that builds it.
    pub(super) fn index_pending(&self) -> bool {
        self.wrapped.on && self.wrapped.draw.is_none()
    }

    /// The pair row list item `item` shows.
    pub(super) fn pair_row(&self, item: usize) -> usize {
        self.wrapped
            .draw
            .as_ref()
            .map_or(item, |wrap| wrap.index.at(item).0)
    }

    /// The list's item count for a layout of `rows` pair rows.
    pub(super) fn items(&self, rows: usize) -> usize {
        self.wrapped
            .draw
            .as_ref()
            .map_or(rows, |wrap| wrap.index.total())
    }

    /// The wrap the rows closure draws with; `None` while flat.
    pub(super) fn wrap_draw(&self) -> Option<WrapDraw> {
        self.wrapped.draw.clone()
    }
}

#[cfg(test)]
impl FileView {
    /// The wrap index and width (columns), the top list item and the items the list has room
    /// for, at rows `row_height` px high.
    pub(in crate::window_view) fn wrap_seen(
        &self,
        row_height: Pixels,
    ) -> (Option<WrapIndex>, Option<u32>, usize, usize) {
        let wrap = self.wrapped.draw.as_ref();
        let height = f32::from(self.scroll.viewport_height()) / f32::from(row_height);
        (
            wrap.map(|wrap| wrap.index.clone()),
            wrap.map(|wrap| wrap.cols),
            self.scroll.top_row(row_height),
            height.floor() as usize,
        )
    }
}
