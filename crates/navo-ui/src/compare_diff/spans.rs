//! The changed spans inside lines (Rich look): which rows were asked for, and the spans that
//! came back, keyed by line so both layouts can paint them. At most 4,096 rows are kept.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;

use super::hooks::{DiffRow, PairText, RowSpans};

/// The most rows whose spans are kept; the oldest asked rows go first.
pub const MAX_ROWS: usize = 4096;

/// Rows asked for, and the spans found for them. Rows with no spans count as known too.
#[derive(Debug, Default)]
pub struct SpanCache {
    asked: HashSet<u32>,
    order: VecDeque<u32>,
    left: HashMap<u32, Vec<Range<u32>>>,
    right: HashMap<u32, Vec<Range<u32>>>,
    /// The lines each row with spans filled, so evicting the row frees them.
    lines: HashMap<u32, (u32, u32)>,
}

impl SpanCache {
    /// The smallest range that covers the rows of `rows` not asked yet, or `None` when all are
    /// known. The range is marked asked. `rows` is cut to `MAX_ROWS` rows from its start.
    pub fn request(&mut self, rows: Range<u32>) -> Option<Range<u32>> {
        let end = rows.end.min(rows.start.saturating_add(MAX_ROWS as u32));
        let mut unknown = (rows.start..end).filter(|row| !self.asked.contains(row));
        let first = unknown.next()?;
        let last = unknown.next_back().unwrap_or(first);
        for row in first..=last {
            if self.asked.insert(row) {
                self.order.push_back(row);
            }
        }
        self.evict();
        Some(first..last + 1)
    }

    /// Stores the spans that arrived for `text`; rows that are not changed rows are ignored.
    pub fn insert(&mut self, text: &PairText, spans: Vec<RowSpans>) {
        for found in spans {
            let at = usize::try_from(found.row).ok();
            let Some(DiffRow::Changed(left, right)) = at.and_then(|at| text.rows.get(at)) else {
                continue;
            };
            self.left.insert(*left, found.left);
            self.right.insert(*right, found.right);
            self.lines.insert(found.row, (*left, *right));
        }
    }

    /// The changed byte ranges of left line `line`.
    #[must_use]
    pub fn left(&self, line: u32) -> &[Range<u32>] {
        self.left.get(&line).map_or(&[], Vec::as_slice)
    }

    /// The changed byte ranges of right line `line`.
    #[must_use]
    pub fn right(&self, line: u32) -> &[Range<u32>] {
        self.right.get(&line).map_or(&[], Vec::as_slice)
    }

    /// How many rows are known.
    #[must_use]
    pub fn known(&self) -> usize {
        self.asked.len()
    }

    /// Drops the oldest rows until at most `MAX_ROWS` remain.
    fn evict(&mut self) {
        while self.order.len() > MAX_ROWS {
            let Some(row) = self.order.pop_front() else {
                break;
            };
            self.asked.remove(&row);
            if let Some((left, right)) = self.lines.remove(&row) {
                self.left.remove(&left);
                self.right.remove(&right);
            }
        }
    }
}

#[cfg(test)]
mod tests;
