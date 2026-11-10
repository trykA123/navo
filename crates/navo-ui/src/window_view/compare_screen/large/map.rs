//! The row map of a large text diff (D16): which lines each list row shows, built from the
//! sorted hunk list alone, so a 10M-line pair costs one segment per hunk, never per line.

use std::ops::Range;

use crate::compare_diff::{LargeHunk, Layout, ShownKind};

/// A line on one side; the view keeps it on top while hunks arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) enum Anchor {
    Left(u64),
    Right(u64),
}

/// What one list row shows: its kind, its line on each side and the change it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) struct Shown {
    pub(in crate::window_view) kind: ShownKind,
    pub(in crate::window_view) left: Option<u64>,
    pub(in crate::window_view) right: Option<u64>,
    pub(in crate::window_view) hunk: Option<usize>,
}

/// What a run of rows is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// Equal lines, paired one to one.
    Equal,
    /// Hunk number `n` of the sorted list.
    Hunk(usize),
    /// A gap whose sides differ in length (an evicted section): one changed block.
    Block,
}

/// A run of rows starting at list row `row` over `left` and `right` lines.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    row: u64,
    left: Range<u64>,
    right: Range<u64>,
    part: Part,
}

/// Rows past the known lines while the line counts are still unknown.
const OPEN_TAIL: u64 = 2_000;

/// The rows of a large pair in one layout.
#[derive(Debug, Clone)]
pub(in crate::window_view) struct RowMap {
    hunks: Vec<LargeHunk>,
    segments: Vec<Segment>,
    layout: Layout,
    totals: Option<(u64, u64)>,
}

/// A range's length.
fn len(range: &Range<u64>) -> u64 {
    range.end.saturating_sub(range.start)
}

impl RowMap {
    /// Sorts `hunks` by line and maps them in `layout`; `totals` are both sides' line counts,
    /// unknown until the first line window arrives. Empty or overlapping hunks are skipped.
    pub(in crate::window_view) fn new(
        hunks: &[LargeHunk],
        layout: Layout,
        totals: Option<(u64, u64)>,
    ) -> Self {
        let mut sorted = hunks.to_vec();
        sorted.sort_by_key(|hunk| (hunk.left_rows.start, hunk.right_rows.start));
        let mut kept: Vec<LargeHunk> = Vec::with_capacity(sorted.len());
        for hunk in sorted {
            let after = kept.last().is_none_or(|last| {
                hunk.left_rows.start >= last.left_rows.end
                    && hunk.right_rows.start >= last.right_rows.end
            });
            if after && (len(&hunk.left_rows) > 0 || len(&hunk.right_rows) > 0) {
                kept.push(hunk);
            }
        }
        let mut map = Self {
            hunks: kept,
            segments: Vec::new(),
            layout,
            totals,
        };
        map.segments = map.build();
        map
    }

    /// The segments: a gap before each hunk, the hunk, then the tail to the line counts.
    fn build(&self) -> Vec<Segment> {
        let mut segments = Vec::with_capacity(self.hunks.len() * 2 + 1);
        let (mut row, mut left, mut right) = (0u64, 0u64, 0u64);
        let mut push = |segment: Segment, row: &mut u64| {
            *row = row.saturating_add(self.rows_of(&segment));
            segments.push(segment);
        };
        for (n, hunk) in self.hunks.iter().enumerate() {
            let gap = gap(
                left..hunk.left_rows.start,
                right..hunk.right_rows.start,
                row,
            );
            if let Some(gap) = gap {
                push(gap, &mut row);
            }
            let part = Part::Hunk(n);
            let (l, r) = (hunk.left_rows.clone(), hunk.right_rows.clone());
            push(
                Segment {
                    row,
                    left: l,
                    right: r,
                    part,
                },
                &mut row,
            );
            (left, right) = (hunk.left_rows.end, hunk.right_rows.end);
        }
        let tail = match self.totals {
            Some((l, r)) => gap(left..l.max(left), right..r.max(right), row),
            None => Some(Segment {
                row,
                left: left..u64::MAX,
                right: right..u64::MAX,
                part: Part::Equal,
            }),
        };
        if let Some(tail) = tail {
            push(tail, &mut row);
        }
        segments
    }

    /// How many rows a segment takes in this layout.
    fn rows_of(&self, segment: &Segment) -> u64 {
        let (l, r) = (len(&segment.left), len(&segment.right));
        match (segment.part, self.layout) {
            (Part::Equal, _) => l.min(r),
            (_, Layout::SideBySide) => l.max(r),
            (_, Layout::Unified) => l.saturating_add(r),
        }
    }

    /// The sorted hunks the map was built from.
    pub(in crate::window_view) fn hunks(&self) -> &[LargeHunk] {
        &self.hunks
    }

    /// The layout the rows are counted in.
    pub(in crate::window_view) fn layout(&self) -> Layout {
        self.layout
    }

    /// Both sides' line counts, once known.
    pub(in crate::window_view) fn totals(&self) -> Option<(u64, u64)> {
        self.totals
    }

    /// The list's row count; with unknown line counts, 2,000 rows past the last hunk.
    pub(in crate::window_view) fn count(&self) -> u64 {
        match (self.totals, self.segments.last()) {
            (Some(_), Some(last)) => last.row.saturating_add(self.rows_of(last)),
            (None, Some(last)) => last.row.saturating_add(OPEN_TAIL),
            (_, None) => 0,
        }
    }

    /// The first row of hunk `n`.
    pub(in crate::window_view) fn hunk_row(&self, n: usize) -> Option<u64> {
        self.segments
            .iter()
            .find(|segment| segment.part == Part::Hunk(n))
            .map(|segment| segment.row)
    }

    /// What list row `row` shows; past the end it shows nothing.
    pub(in crate::window_view) fn at(&self, row: u64) -> Shown {
        let found = self.segments.partition_point(|segment| segment.row <= row);
        let Some(segment) = found.checked_sub(1).and_then(|at| self.segments.get(at)) else {
            return blank();
        };
        let k = row - segment.row;
        if k >= self.rows_of(segment) {
            return blank();
        }
        let line = |range: &Range<u64>, k: u64| (k < len(range)).then(|| range.start + k);
        let (left, right) = match (segment.part, self.layout) {
            (Part::Equal, _) | (_, Layout::SideBySide) => {
                (line(&segment.left, k), line(&segment.right, k))
            }
            (_, Layout::Unified) => match line(&segment.left, k) {
                Some(at) => (Some(at), None),
                None => (None, line(&segment.right, k - len(&segment.left))),
            },
        };
        let kind = match (segment.part, left, right) {
            (Part::Equal, ..) => ShownKind::Equal,
            (_, Some(_), Some(_)) => ShownKind::Changed,
            (_, Some(_), None) => ShownKind::Deleted,
            (_, None, _) => ShownKind::Inserted,
        };
        let hunk = match segment.part {
            Part::Hunk(n) => Some(n),
            Part::Equal | Part::Block => None,
        };
        Shown {
            kind,
            left,
            right,
            hunk,
        }
    }

    /// The row that shows `anchor`'s line; a line past the end gives the last row.
    pub(in crate::window_view) fn row_of(&self, anchor: Anchor) -> u64 {
        let (line, side_of): (u64, fn(&Segment) -> &Range<u64>) = match anchor {
            Anchor::Left(line) => (line, |segment| &segment.left),
            Anchor::Right(line) => (line, |segment| &segment.right),
        };
        let at = self
            .segments
            .partition_point(|segment| side_of(segment).end <= line);
        let Some(segment) = self.segments.get(at) else {
            return self.count().saturating_sub(1);
        };
        let range = side_of(segment);
        let k = line.saturating_sub(range.start);
        let before = match (anchor, segment.part, self.layout) {
            (Anchor::Right(_), Part::Hunk(_) | Part::Block, Layout::Unified) => len(&segment.left),
            _ => 0,
        };
        segment.row.saturating_add(before).saturating_add(k)
    }

    /// The top line of `row`, to keep on top: the left line, else the right one.
    pub(in crate::window_view) fn anchor(&self, row: u64) -> Option<Anchor> {
        let shown = self.at(row);
        shown
            .left
            .map(Anchor::Left)
            .or_else(|| shown.right.map(Anchor::Right))
    }
}

/// The gap between two hunks starting at `row`: equal when both sides are as long, else one
/// changed block; `None` when both are empty.
fn gap(left: Range<u64>, right: Range<u64>, row: u64) -> Option<Segment> {
    let part = match (len(&left), len(&right)) {
        (0, 0) => return None,
        (l, r) if l == r => Part::Equal,
        _ => Part::Block,
    };
    Some(Segment {
        row,
        left,
        right,
        part,
    })
}

/// A row past the end: a filler with no lines.
fn blank() -> Shown {
    Shown {
        kind: ShownKind::Filler,
        left: None,
        right: None,
        hunk: None,
    }
}
