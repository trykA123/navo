//! Row math of the hex body: offset rows, content rows that split only when the two sides
//! start a changed region at different lengths, and the row each region starts on.

use navo_core::paging::HEX_WIDTH;

use crate::compare_diff::{ByteRegionView, HexAlign};

/// The byte offset and count a side shows in one row; the count is at most 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::window_view) struct RowSpan {
    pub(super) at: u64,
    pub(super) len: usize,
}

impl RowSpan {
    /// The number of bytes this side shows.
    pub(super) fn len(self) -> usize {
        self.len
    }
}

/// One aligned row: the bytes each side shows, or `None` for a gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view) struct RowAt {
    pub(super) left: Option<RowSpan>,
    pub(super) right: Option<RowSpan>,
}

impl RowAt {
    /// Whether a side has no bytes in this row.
    #[cfg(test)]
    pub(super) fn is_gap(self) -> bool {
        self.left.is_none() || self.right.is_none()
    }
}

/// The offsets and byte lengths of one run of rows.
#[derive(Debug, Clone, Copy)]
struct Run {
    left: u64,
    right: u64,
    left_len: u64,
    right_len: u64,
}

/// A run of rows that start at the same pair of offsets, with each side's byte length.
#[derive(Debug, Clone, Copy)]
struct Segment {
    first: usize,
    left: u64,
    right: u64,
    left_len: u64,
    right_len: u64,
}

/// The aligned rows and the row each changed region starts on.
#[derive(Debug, Clone, Default)]
pub(in crate::window_view) struct Rows {
    segments: Vec<Segment>,
    total: usize,
    region_rows: Vec<usize>,
}

impl Rows {
    /// Rows by offset over `lengths`; the longer side's tail shows gaps.
    pub(super) fn offset(lengths: (u64, u64)) -> Self {
        let mut rows = Self::default();
        rows.push_segment(Run {
            left: 0,
            right: 0,
            left_len: lengths.0,
            right_len: lengths.1,
        });
        rows
    }

    /// Rows by content. Rows split only where the sides shift: an inserted or deleted block,
    /// or a changed region whose two lengths differ. Equal-length edits stay in lockstep.
    pub(super) fn content(lengths: (u64, u64), regions: &[ByteRegionView]) -> Self {
        let mut rows = Self::default();
        let (mut left, mut right) = (0, 0);
        let (mut run_left, mut run_right, mut run_row) = (0, 0, 0usize);
        for region in regions {
            let at_left = region.left.start.max(left);
            let at_right = region.right.start.max(right);
            let left_len = region.left.end.saturating_sub(at_left);
            let right_len = region.right.end.saturating_sub(at_right);
            let aligned = at_left - run_left == at_right - run_right && left_len == right_len;
            if aligned {
                let row = run_row.saturating_add(row_of(at_left.saturating_sub(run_left)));
                rows.region_rows.push(row);
            } else {
                run_row = rows.split(
                    Run {
                        left: run_left,
                        right: run_right,
                        left_len: at_left.saturating_sub(run_left),
                        right_len: at_right.saturating_sub(run_right),
                    },
                    Run {
                        left: at_left,
                        right: at_right,
                        left_len,
                        right_len,
                    },
                );
                run_left = at_left.saturating_add(left_len);
                run_right = at_right.saturating_add(right_len);
            }
            left = at_left.saturating_add(left_len);
            right = at_right.saturating_add(right_len);
        }
        rows.push_segment(Run {
            left: run_left,
            right: run_right,
            left_len: lengths.0.saturating_sub(run_left),
            right_len: lengths.1.saturating_sub(run_right),
        });
        rows
    }

    /// How many rows the pair has.
    pub(super) fn count(&self) -> usize {
        self.total
    }

    /// The bytes of row `index`, or `None` past the end.
    pub(super) fn row(&self, index: usize) -> Option<RowAt> {
        if index >= self.total {
            return None;
        }
        let at = self
            .segments
            .partition_point(|segment| segment.first <= index);
        let segment = self.segments.get(at.checked_sub(1)?)?;
        let step = u64::try_from(index - segment.first)
            .ok()?
            .checked_mul(HEX_WIDTH)?;
        Some(RowAt {
            left: span(segment.left, step, segment.left_len),
            right: span(segment.right, step, segment.right_len),
        })
    }

    /// The left byte offset where row `index` starts; `None` for a gap or past the end.
    pub(super) fn left_at(&self, index: usize) -> Option<u64> {
        self.row(index)?.left.map(|span| span.at)
    }

    /// The left byte offset row `index` starts at; a gap row takes the next row's that has a
    /// left side, or the end of the left bytes when none follows.
    pub(super) fn left_from(&self, index: usize) -> u64 {
        if let Some(at) = self.left_at(index) {
            return at;
        }
        let after = self
            .segments
            .partition_point(|segment| segment.first <= index);
        let shows_left = |segment: &&Segment| segment.left_len > 0;
        let next = self.segments.get(after..).unwrap_or_default();
        match next.iter().find(shows_left) {
            Some(segment) => segment.left,
            None => self
                .segments
                .iter()
                .rev()
                .find(shows_left)
                .map_or(0, |segment| segment.left.saturating_add(segment.left_len)),
        }
    }

    /// The first row that shows left byte `offset` or later; the last row past the end.
    pub(super) fn row_of_left(&self, offset: u64) -> usize {
        let at = self
            .segments
            .iter()
            .position(|segment| offset < segment.left.saturating_add(segment.left_len));
        let Some(segment) = at.and_then(|at| self.segments.get(at)) else {
            return self.total.saturating_sub(1);
        };
        let inside = offset.saturating_sub(segment.left) / HEX_WIDTH;
        let row = segment
            .first
            .saturating_add(usize::try_from(inside).unwrap_or(0));
        row.min(self.total.saturating_sub(1))
    }

    /// The row region `at` starts on; the last row when the region has no bytes.
    pub(super) fn row_for_region(
        &self,
        at: usize,
        regions: &[ByteRegionView],
        align: HexAlign,
    ) -> Option<usize> {
        let region = regions.get(at)?;
        let row = match align {
            HexAlign::Content => self.region_rows.get(at).copied(),
            HexAlign::Offset => usize::try_from(region.left.start / HEX_WIDTH).ok(),
        }?;
        Some(row.min(self.total.saturating_sub(1)))
    }

    /// Splits at a shifted region: the equal run before it, then the region itself.
    fn split(&mut self, gap: Run, region: Run) -> usize {
        if gap.left_len > 0 || gap.right_len > 0 {
            self.push_segment(gap);
        }
        self.region_rows.push(self.total);
        self.push_segment(region);
        self.total
    }

    /// Adds one run of rows unless it is empty.
    fn push_segment(&mut self, run: Run) {
        let widest = run.left_len.max(run.right_len);
        let rows = usize::try_from(widest.div_ceil(HEX_WIDTH)).unwrap_or(usize::MAX);
        if rows == 0 {
            return;
        }
        self.segments.push(Segment {
            first: self.total,
            left: run.left,
            right: run.right,
            left_len: run.left_len,
            right_len: run.right_len,
        });
        self.total = self.total.saturating_add(rows);
    }
}

/// The row a byte offset falls on inside a run.
fn row_of(offset: u64) -> usize {
    usize::try_from(offset / HEX_WIDTH).unwrap_or(usize::MAX)
}

/// The span `step` bytes into a run of `len` bytes, or `None` past its end.
fn span(start: u64, step: u64, len: u64) -> Option<RowSpan> {
    if step >= len {
        return None;
    }
    let left = len - step;
    Some(RowSpan {
        at: start + step,
        len: usize::try_from(left.min(HEX_WIDTH)).unwrap_or(1),
    })
}

/// A region range, for tests that build content rows directly.
#[cfg(test)]
pub(super) fn region(left: std::ops::Range<u64>, right: std::ops::Range<u64>) -> ByteRegionView {
    ByteRegionView {
        changed: left.end.saturating_sub(left.start) + right.end.saturating_sub(right.start),
        left,
        right,
    }
}

#[cfg(test)]
mod tests {
    use super::{Rows, region};
    use crate::compare_diff::HexAlign;

    #[test]
    fn offset_rows_run_to_the_longer_side_and_gap_its_tail() {
        let rows = Rows::offset((16, 32));
        assert_eq!(rows.count(), 2);
        assert_eq!(
            rows.row(0)
                .map(|row| (row.left.map(|s| s.at), row.right.map(|s| s.at))),
            Some((Some(0), Some(0)))
        );
        assert_eq!(
            rows.row(1)
                .map(|row| (row.left.map(|s| s.at), row.right.map(|s| s.at))),
            Some((None, Some(16)))
        );
        assert!(rows.row(2).is_none());
    }

    #[test]
    fn content_rows_put_a_gap_at_an_insertion_and_keep_the_rest() {
        let rows = Rows::content((16, 24), &[region(8..8, 8..16)]);
        assert_eq!(rows.count(), 3);
        assert!(!rows.row(0).unwrap().is_gap());
        assert!(rows.row(1).unwrap().is_gap());
        assert_eq!(
            rows.row(2)
                .map(|row| (row.left.map(|s| s.at), row.right.map(|s| s.at))),
            Some((Some(8), Some(16)))
        );
    }

    #[test]
    fn equal_length_edits_do_not_split_rows_or_bleed_into_the_next_region() {
        let rows = Rows::content((24, 24), &[region(4..8, 4..8), region(12..16, 12..16)]);
        assert_eq!(rows.count(), 2);
        assert_eq!(
            rows.row(0).and_then(|row| row.left),
            Some(super::RowSpan { at: 0, len: 16 })
        );
        assert_eq!(
            rows.row(1).and_then(|row| row.left),
            Some(super::RowSpan { at: 16, len: 8 })
        );
        assert_eq!(
            rows.row_for_region(
                1,
                &[region(4..8, 4..8), region(12..16, 12..16)],
                HexAlign::Content
            ),
            Some(0)
        );
    }

    #[test]
    fn a_short_segment_row_carries_its_own_length() {
        let rows = Rows::content((8, 24), &[region(8..8, 8..16)]);
        assert_eq!(rows.count(), 3);
        assert_eq!(
            rows.row(0).and_then(|row| row.left),
            Some(super::RowSpan { at: 0, len: 8 })
        );
    }
}
