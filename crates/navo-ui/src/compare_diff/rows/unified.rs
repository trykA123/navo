//! Unified rows: inside a hunk, each run of changed rows lists its deleted lines, then its
//! inserted lines. Rows between hunks are skipped by arithmetic, so a frame costs the hunks'
//! rows before the range plus the range itself.

use std::ops::Range;

use super::{ShownKind, ShownRow, line, side_row};
use crate::compare_diff::hooks::{DiffRow, PairText};

/// The number of unified rows of `text`.
pub(super) fn count(text: &PairText) -> usize {
    let mut cut = Cut {
        text,
        shown: 0,
        range: usize::MAX..usize::MAX,
        out: Vec::new(),
    };
    cut.walk();
    cut.shown
}

/// The unified rows in `range`, clamped to the pair.
pub(super) fn rows(text: &PairText, range: Range<usize>) -> Vec<ShownRow<'_>> {
    let mut cut = Cut {
        text,
        shown: 0,
        out: Vec::with_capacity(range.len().min(4096)),
        range,
    };
    cut.walk();
    cut.out
}

/// A walk over the unified rows that keeps only those inside `range`.
struct Cut<'a> {
    text: &'a PairText,
    /// Unified rows passed so far.
    shown: usize,
    range: Range<usize>,
    out: Vec<ShownRow<'a>>,
}

impl<'a> Cut<'a> {
    /// Walks the hunks in order; stops once the range is full.
    fn walk(&mut self) {
        let text = self.text;
        let len = text.rows.len();
        let mut pos = 0;
        for (index, hunk) in text.hunks.iter().enumerate() {
            let start = usize::try_from(hunk.rows.start).map_or(len, |s| s.clamp(pos, len));
            let end = usize::try_from(hunk.rows.end).map_or(len, |e| e.clamp(start, len));
            if !self.plain(pos..start) || !self.hunk(index, start..end) {
                return;
            }
            pos = end;
        }
        self.plain(pos..len);
    }

    /// Rows outside any hunk, one shown row each. Returns false once the range is full.
    fn plain(&mut self, rows: Range<usize>) -> bool {
        let skip = self.range.start.saturating_sub(self.shown).min(rows.len());
        self.shown += skip;
        for at in rows.start + skip..rows.end {
            if !self.push(side_row(self.text, at, None)) {
                return false;
            }
        }
        true
    }

    /// The rows of hunk `index`. Returns false once the range is full.
    fn hunk(&mut self, index: usize, rows: Range<usize>) -> bool {
        let text = self.text;
        let hunk = u32::try_from(index).ok();
        let mut at = rows.start;
        let equal = |i: usize| matches!(text.rows[i], DiffRow::Equal(..));
        while at < rows.end {
            let (next, more) = if equal(at) {
                (at + 1, self.push(side_row(text, at, hunk)))
            } else {
                let end = (at..rows.end).find(|&i| equal(i)).unwrap_or(rows.end);
                (end, self.run(hunk, at..end))
            };
            if !more {
                return false;
            }
            at = next;
        }
        true
    }

    /// One run of changed rows: all its left lines as deleted, then all its right lines as
    /// inserted. A run wholly before the range is only counted. Returns false once full.
    fn run(&mut self, hunk: Option<u32>, rows: Range<usize>) -> bool {
        let base = rows.start;
        let sides = &self.text.rows[rows];
        let (lefts, rights) = sides.iter().fold((0, 0), |(l, r), row| {
            (
                l + usize::from(left(*row).is_some()),
                r + usize::from(right(*row).is_some()),
            )
        });
        self.side((sides, base), hunk, Side::Left(lefts))
            && self.side((sides, base), hunk, Side::Right(rights))
    }

    /// The lines of one side of a run whose first row is `DiffRow` number `base`. Lines before
    /// the range are skipped by index only, so a long run costs one cheap scan. Returns false
    /// once the range is full.
    fn side(&mut self, (sides, base): (&[DiffRow], usize), hunk: Option<u32>, side: Side) -> bool {
        let text = self.text;
        let (pick, count): (fn(DiffRow) -> Option<u32>, usize) = match side {
            Side::Left(count) => (left, count),
            Side::Right(count) => (right, count),
        };
        let skip = self.range.start.saturating_sub(self.shown).min(count);
        self.shown += skip;
        if skip == count {
            return true;
        }
        let picked = sides.iter().enumerate();
        let picked = picked.filter_map(|(at, row)| Some((base + at, pick(*row)?)));
        for (at, index) in picked.skip(skip) {
            let row = u32::try_from(at).unwrap_or(u32::MAX);
            let shown = match side {
                Side::Left(_) => ShownRow {
                    kind: ShownKind::Deleted,
                    left: line(&text.left, index),
                    right: None,
                    hunk,
                    row,
                },
                Side::Right(_) => ShownRow {
                    kind: ShownKind::Inserted,
                    left: None,
                    right: line(&text.right, index),
                    hunk,
                    row,
                },
            };
            if !self.push(shown) {
                return false;
            }
        }
        true
    }

    /// Counts one row and keeps it when inside the range. Returns false once the range is full.
    fn push(&mut self, row: ShownRow<'a>) -> bool {
        if self.shown >= self.range.end {
            return false;
        }
        if self.shown >= self.range.start {
            self.out.push(row);
        }
        self.shown += 1;
        self.shown < self.range.end
    }
}

/// Which side of a run is listed, with its number of lines.
#[derive(Clone, Copy)]
enum Side {
    Left(usize),
    Right(usize),
}

/// The left line index of a changed row, if it has one.
pub(super) fn left(row: DiffRow) -> Option<u32> {
    match row {
        DiffRow::Changed(l, _) | DiffRow::Deleted(l) => Some(l),
        DiffRow::Equal(..) | DiffRow::Inserted(_) => None,
    }
}

/// The right line index of a changed row, if it has one.
pub(super) fn right(row: DiffRow) -> Option<u32> {
    match row {
        DiffRow::Changed(_, r) | DiffRow::Inserted(r) => Some(r),
        DiffRow::Equal(..) | DiffRow::Deleted(_) => None,
    }
}
