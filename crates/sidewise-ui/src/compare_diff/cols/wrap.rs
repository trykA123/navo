//! The wrap index (D39.6): where each pair row starts in the list when long lines wrap.
//! Pure; the file diff builds it once per layout and column width.

use std::ops::Range;
use std::sync::Arc;

use super::{LineCols, visual_lines};
use crate::compare_diff::hooks::{DiffRow, Layout, PairText};
use crate::compare_diff::row_count;

/// Maps list rows (the rows a layout draws) to list items (one item per visual line).
///
/// It stores only the rows taller than one item, so it costs 16 bytes per tall row and every
/// lookup is a binary search. A clone shares that list, so callers may copy it freely.
#[derive(Debug, Clone)]
pub struct WrapIndex {
    rows: usize,
    total: usize,
    tall: Arc<[Tall]>,
}

/// A row that takes more than one item, with the extra items of all tall rows up to and
/// including it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tall {
    row: u32,
    extra: u32,
    cum: usize,
}

impl Tall {
    /// The first item of this row.
    fn first(self) -> usize {
        self.row as usize + self.cum - self.extra as usize
    }
}

/// Collects the heights of rows in list order.
#[derive(Default)]
struct Acc {
    rows: usize,
    extras: usize,
    tall: Vec<Tall>,
}

impl Acc {
    /// Adds the next row, `lines` items high.
    fn push(&mut self, lines: u32) {
        let extra = lines.saturating_sub(1);
        if extra > 0 {
            self.extras += extra as usize;
            let row = u32::try_from(self.rows).unwrap_or(u32::MAX);
            self.tall.push(Tall {
                row,
                extra,
                cum: self.extras,
            });
        }
        self.rows += 1;
    }

    /// Freezes the rows into an index.
    fn finish(self) -> WrapIndex {
        WrapIndex {
            rows: self.rows,
            total: self.rows + self.extras,
            tall: self.tall.into(),
        }
    }
}

/// The left and right line index of a row.
fn sides(row: DiffRow) -> (Option<u32>, Option<u32>) {
    match row {
        DiffRow::Equal(l, r) | DiffRow::Changed(l, r) => (Some(l), Some(r)),
        DiffRow::Deleted(l) => (Some(l), None),
        DiffRow::Inserted(r) => (None, Some(r)),
    }
}

impl WrapIndex {
    /// An index where every one of `rows` rows is one item.
    #[must_use]
    pub fn flat(rows: usize) -> Self {
        Acc {
            rows,
            ..Acc::default()
        }
        .finish()
    }

    /// Builds the index of `text` in `layout` for columns `wrap` wide, from the rows and the
    /// measured `cols`, in one pass. Side by side, a row takes the taller side; unified, it
    /// takes the side that row draws. A line over `LONG_LINE` counts as one.
    #[must_use]
    pub fn build(text: &PairText, layout: Layout, cols: &LineCols, wrap: u32) -> Self {
        let mut acc = Acc::default();
        match layout {
            Layout::SideBySide => side_by_side(&text.rows, (cols, wrap), &mut acc),
            Layout::Unified => Unified::new(text, (cols, wrap), &mut acc).walk(),
        }
        debug_assert_eq!(
            acc.rows,
            row_count(text, layout),
            "rows as `pair_rows` counts them"
        );
        acc.finish()
    }

    /// The number of items.
    #[must_use]
    pub fn total(&self) -> usize {
        self.total
    }

    /// The first item of `row`; `total()` when `row` is past the last row.
    #[must_use]
    pub fn first_of(&self, row: usize) -> usize {
        if row >= self.rows {
            return self.total;
        }
        let before = self.tall.partition_point(|t| (t.row as usize) < row);
        row + self.extras_before(before)
    }

    /// The row that holds `item` and the item's line within it. An item past the end reads as
    /// the last item; an empty index reads `(0, 0)`.
    #[must_use]
    pub fn at(&self, item: usize) -> (usize, u32) {
        let Some(last) = self.total.checked_sub(1) else {
            return (0, 0);
        };
        let item = item.min(last);
        let started = self.tall.partition_point(|t| t.first() <= item);
        if let Some(tall) = started.checked_sub(1).map(|i| self.tall[i]) {
            let sub = item - tall.first();
            if sub <= tall.extra as usize {
                return (tall.row as usize, sub as u32);
            }
        }
        (item - self.extras_before(started), 0)
    }

    /// The items `row` takes; 0 when `row` is past the last row.
    #[must_use]
    pub fn lines_of(&self, row: usize) -> u32 {
        if row >= self.rows {
            return 0;
        }
        let found = u32::try_from(row)
            .ok()
            .and_then(|row| self.tall.binary_search_by_key(&row, |t| t.row).ok());
        found.map_or(1, |i| 1 + self.tall[i].extra)
    }

    /// The extra items of the first `count` tall rows.
    fn extras_before(&self, count: usize) -> usize {
        count.checked_sub(1).map_or(0, |i| self.tall[i].cum)
    }
}

/// The visual lines of line `index` of a side, read through `side`.
fn lines_at(side: fn(&LineCols, u32) -> u32, (cols, wrap): (&LineCols, u32), index: u32) -> u32 {
    visual_lines(side(cols, index), wrap)
}

/// Side by side: one row per `DiffRow`, as tall as the taller of its lines.
fn side_by_side(rows: &[DiffRow], at: (&LineCols, u32), acc: &mut Acc) {
    for row in rows {
        let (left, right) = sides(*row);
        let left = left.map_or(1, |i| lines_at(LineCols::left, at, i));
        let right = right.map_or(1, |i| lines_at(LineCols::right, at, i));
        acc.push(left.max(right));
    }
}

/// A walk over the unified rows, in the order `pair_rows` draws them.
struct Unified<'a> {
    text: &'a PairText,
    at: (&'a LineCols, u32),
    acc: &'a mut Acc,
}

impl<'a> Unified<'a> {
    /// Starts a walk that adds its rows to `acc`.
    fn new(text: &'a PairText, at: (&'a LineCols, u32), acc: &'a mut Acc) -> Self {
        Self { text, at, acc }
    }

    /// Walks the hunks in order, with the plain rows between them.
    fn walk(&mut self) {
        let len = self.text.rows.len();
        let mut pos = 0;
        for hunk in &self.text.hunks {
            let start = usize::try_from(hunk.rows.start).map_or(len, |s| s.clamp(pos, len));
            let end = usize::try_from(hunk.rows.end).map_or(len, |e| e.clamp(start, len));
            self.rows(pos..start, false);
            self.rows(start..end, true);
            pos = end;
        }
        self.rows(pos..len, false);
    }

    /// The rows in `range`. Inside a hunk, a run of changed rows lists its left lines, then its
    /// right lines; every other row is one item row that draws its right side, else its left.
    fn rows(&mut self, range: Range<usize>, in_hunk: bool) {
        let mut at = range.start;
        while at < range.end {
            let row = self.text.rows[at];
            if !in_hunk || matches!(row, DiffRow::Equal(..)) {
                self.single(row);
                at += 1;
                continue;
            }
            let rest = &self.text.rows[at..range.end];
            let run = rest.iter().position(|r| matches!(r, DiffRow::Equal(..)));
            let run = &rest[..run.unwrap_or(rest.len())];
            self.run(run);
            at += run.len();
        }
    }

    /// One row drawn alone: the right line if it has one, else the left.
    fn single(&mut self, row: DiffRow) {
        let lines = match sides(row) {
            (_, Some(r)) => lines_at(LineCols::right, self.at, r),
            (Some(l), None) => lines_at(LineCols::left, self.at, l),
            (None, None) => 1,
        };
        self.acc.push(lines);
    }

    /// A run of changed rows: all its left lines as deleted, then all its right lines.
    fn run(&mut self, run: &[DiffRow]) {
        for row in run {
            if let (Some(l), _) = sides(*row) {
                self.acc.push(lines_at(LineCols::left, self.at, l));
            }
        }
        for row in run {
            if let (_, Some(r)) = sides(*row) {
                self.acc.push(lines_at(LineCols::right, self.at, r));
            }
        }
    }
}

#[cfg(test)]
mod tests;
