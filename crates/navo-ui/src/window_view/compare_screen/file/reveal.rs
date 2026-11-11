//! Scrolls the file diff to a picked line (V32). Side by side the line's row is its `DiffRow`;
//! unified lists a run's deleted lines before its inserted ones, so the row is found by walking.

use gpui_kit::Pixels;

use super::FileView;
use crate::compare_diff::{DiffRow, Layout, PairText, PickSide, line_on};
use crate::window_view::compare_screen::pick::Spot;

/// The list row of the line `spot` in unified layout; `start` is its change's first row.
fn unified_row(text: &PairText, start: usize, spot: Spot) -> usize {
    let Some(view) = text.hunks.get(spot.hunk as usize) else {
        return start;
    };
    let range = view.rows.start as usize..view.rows.end as usize;
    let rows = text.rows.get(range).unwrap_or_default();
    let (mut at, mut i) = (start, 0);
    while i < rows.len() {
        if matches!(rows[i], DiffRow::Equal(..)) {
            (at, i) = (at + 1, i + 1);
            continue;
        }
        let len = rows[i..]
            .iter()
            .position(|row| matches!(row, DiffRow::Equal(..)))
            .unwrap_or(rows.len() - i);
        let run = &rows[i..i + len];
        let lines = |side| run.iter().filter_map(move |row| line_on(*row, side));
        let lefts = lines(PickSide::Left).count();
        let found = lines(spot.side).position(|line| line == spot.line);
        match (found, spot.side) {
            (Some(p), PickSide::Left) => return at + p,
            (Some(p), PickSide::Right) => return at + lefts + p,
            (None, _) => {}
        }
        (at, i) = (at + lefts + lines(PickSide::Right).count(), i + len);
    }
    start
}

impl FileView {
    /// The list row of the pickable line `spot` in `layout`.
    fn row_of(&mut self, layout: Layout, spot: Spot) -> usize {
        let start = self.shape(layout).starts.get(spot.hunk as usize).copied();
        match layout {
            Layout::SideBySide => spot.row as usize,
            Layout::Unified => unified_row(&self.text, start.unwrap_or(0), spot),
        }
    }

    /// Scrolls so the picked line `spot` is on screen, moving as little as it can. The list is
    /// asked in items: while lines wrap a row of it starts at its row's first item.
    pub(in crate::window_view::compare_screen) fn reveal(&mut self, layout: Layout, spot: Spot) {
        let row = self.row_of(layout, spot);
        self.scroll.scroll_to(self.list_row(row));
    }

    /// Where the bottom of the cursor line is, in px from the top of the list, given the
    /// pick `spot`, else the current change's first row, else the first row. May be off screen.
    /// A wrapped row ends below its last visual line, so its items are counted.
    pub(in crate::window_view::compare_screen) fn line_bottom(
        &mut self,
        layout: Layout,
        spot: Option<Spot>,
        row_height: Pixels,
    ) -> Pixels {
        let row = match (spot, self.current) {
            (Some(spot), _) => self.row_of(layout, spot),
            (None, Some(hunk)) => self.shape(layout).starts.get(hunk).copied().unwrap_or(0),
            (None, None) => 0,
        };
        let below = self.list_row(row) + self.row_lines(row) as usize;
        self.scroll.offset_y() + row_height * below as f32
    }
}
