//! The display lines a large text diff has read so far, and which window to ask for next.
//! A bounded cache (D16): lines far from the view are dropped, so memory stays flat.

use std::collections::BTreeMap;
use std::ops::Range;

use super::map::RowMap;

/// Lines kept per side; about four full windows.
const KEEP: usize = 8_192;
/// Lines read past the visible ones on each side, so a short scroll finds them ready.
const AHEAD: u64 = 200;
/// The most lines one request reads; the app caps a window at the same count.
const MOST: u64 = 2_000;

/// Both sides' cached lines and the window in flight.
#[derive(Debug, Default)]
pub(in crate::window_view) struct LineCache {
    left: BTreeMap<u64, String>,
    right: BTreeMap<u64, String>,
    /// The window asked for last; cleared when any window arrives.
    asked: Option<Range<u64>>,
}

impl LineCache {
    /// Line `line` of the left (`left`) or right side, if it arrived.
    pub(in crate::window_view) fn line(&self, left: bool, line: u64) -> Option<&str> {
        let side = if left { &self.left } else { &self.right };
        side.get(&line).map(String::as_str)
    }

    /// Stores a window that starts at line `start`, then drops the lines farthest from it.
    pub(in crate::window_view) fn add(
        &mut self,
        start: u64,
        left: Vec<String>,
        right: Vec<String>,
    ) {
        self.asked = None;
        for (side, lines) in [(&mut self.left, left), (&mut self.right, right)] {
            for (at, text) in (start..).zip(lines) {
                side.insert(at, text);
            }
            trim(side, start);
        }
    }

    /// The window to ask for so `rows` show text: the missing part of the left lines, else of
    /// the right lines, widened by up to 200 lines and at most 2,000 long. `None` when all arrived or the
    /// window is already in flight.
    pub(in crate::window_view) fn wanted(
        &self,
        map: &RowMap,
        rows: Range<u64>,
    ) -> Option<Range<u64>> {
        let (mut left, mut right) = (None::<Range<u64>>, None::<Range<u64>>);
        for row in rows {
            let shown = map.at(row);
            widen(&mut left, shown.left);
            widen(&mut right, shown.right);
        }
        let (left_total, right_total) = map.totals().unwrap_or((u64::MAX, u64::MAX));
        let (side, missing) = missing(&self.left, left, left_total)
            .map(|range| (&self.left, range))
            .or_else(|| {
                missing(&self.right, right, right_total).map(|range| (&self.right, range))
            })?;
        if self
            .asked
            .as_ref()
            .is_some_and(|asked| asked.start <= missing.start && missing.end <= asked.end)
        {
            return None;
        }
        // Widen back over missing lines only, so every answer brings at least one new line.
        let mut start = missing.start;
        while start > 0 && missing.start - start < AHEAD && !side.contains_key(&(start - 1)) {
            start -= 1;
        }
        let end = missing
            .end
            .saturating_add(AHEAD)
            .min(start.saturating_add(MOST));
        Some(start..end)
    }

    /// Notes the window just asked for.
    pub(in crate::window_view) fn asked(&mut self, rows: Range<u64>) {
        self.asked = Some(rows);
    }
}

/// Grows `range` to hold `line`.
fn widen(range: &mut Option<Range<u64>>, line: Option<u64>) {
    let Some(line) = line else {
        return;
    };
    *range = Some(match range.take() {
        Some(range) => range.start.min(line)..range.end.max(line + 1),
        None => line..line + 1,
    });
}

/// The lines of `range` below `total` not in `side`, from the first missing to the last.
fn missing(
    side: &BTreeMap<u64, String>,
    range: Option<Range<u64>>,
    total: u64,
) -> Option<Range<u64>> {
    let range = range?;
    let range = range.start..range.end.min(total);
    let first = range.clone().find(|line| !side.contains_key(line))?;
    let last = range.rev().find(|line| !side.contains_key(line))?;
    Some(first..last + 1)
}

/// Drops the lines farthest from `near` until at most 8,192 stay.
fn trim(side: &mut BTreeMap<u64, String>, near: u64) {
    while side.len() > KEEP {
        let low = side.first_key_value().map_or(near, |(at, _)| *at);
        let high = side.last_key_value().map_or(near, |(at, _)| *at);
        if near.saturating_sub(low) > high.saturating_sub(near) {
            side.pop_first();
        } else {
            side.pop_last();
        }
    }
}
