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

/// One fetched line window: its first line, both sides' text and their file byte offsets.
pub(in crate::window_view) struct Fetched {
    pub start: u64,
    pub lines: (Vec<String>, Vec<String>),
    pub starts: (Vec<u64>, Vec<u64>),
}

/// Both sides' cached lines and the window in flight.
#[derive(Debug, Default)]
pub(in crate::window_view) struct LineCache {
    left: BTreeMap<u64, String>,
    right: BTreeMap<u64, String>,
    /// The one window in flight; cleared when it arrives.
    asked: Option<Range<u64>>,
    /// The file byte offset of each cached line, per side (V41d).
    starts_left: BTreeMap<u64, u64>,
    starts_right: BTreeMap<u64, u64>,
}

impl LineCache {
    /// Line `line` of the left (`left`) or right side, if it arrived.
    pub(in crate::window_view) fn line(&self, left: bool, line: u64) -> Option<&str> {
        let side = if left { &self.left } else { &self.right };
        side.get(&line).map(String::as_str)
    }

    /// The file byte offset of line `line` on the left (`left`) or right side, when it is cached.
    pub(in crate::window_view) fn start_of(&self, left: bool, line: u64) -> Option<u64> {
        let starts = if left {
            &self.starts_left
        } else {
            &self.starts_right
        };
        starts.get(&line).copied()
    }

    /// Stores a fetched window, then drops each side's lines farthest from
    /// its line in view (`near`), so sides far apart never evict each other's visible lines.
    pub(in crate::window_view) fn add(&mut self, fetched: Fetched, near: (u64, u64)) {
        let Fetched {
            start,
            lines: (left, right),
            starts: (left_starts, right_starts),
        } = fetched;
        self.asked = None;
        let sides = [
            (
                &mut self.left,
                &mut self.starts_left,
                left,
                left_starts,
                near.0,
            ),
            (
                &mut self.right,
                &mut self.starts_right,
                right,
                right_starts,
                near.1,
            ),
        ];
        for (side, kept, lines, starts, near) in sides {
            for (at, offset) in (start..).zip(starts) {
                kept.insert(at, offset);
            }
            for (at, text) in (start..).zip(lines) {
                side.insert(at, text);
            }
            trim(side, near);
            kept.retain(|line, _| side.contains_key(line));
        }
    }

    /// The window to ask for so `rows` show text: the missing part of the left lines, else of
    /// the right lines, widened by up to 200 lines and at most 2,000 long. `None` when all
    /// arrived or a window is in flight: one at a time, so an answer never races a newer ask.
    pub(in crate::window_view) fn wanted(
        &self,
        map: &RowMap,
        rows: Range<u64>,
    ) -> Option<Range<u64>> {
        if self.asked.is_some() {
            return None;
        }
        let (left, right) = in_view(map, rows);
        let (left_total, right_total) = map.totals().unwrap_or((u64::MAX, u64::MAX));
        let (side, missing) = missing(&self.left, left, left_total)
            .map(|range| (&self.left, range))
            .or_else(|| {
                missing(&self.right, right, right_total).map(|range| (&self.right, range))
            })?;
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

/// The left and right lines that list rows `rows` show.
pub(in crate::window_view) fn in_view(
    map: &RowMap,
    rows: Range<u64>,
) -> (Option<Range<u64>>, Option<Range<u64>>) {
    let (mut left, mut right) = (None::<Range<u64>>, None::<Range<u64>>);
    for row in rows {
        let shown = map.at(row);
        widen(&mut left, shown.left);
        widen(&mut right, shown.right);
    }
    (left, right)
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

/// The lines of `range` below `total` not in `side`, from the first missing to the last missing
/// within one request (2,000 lines). It walks only cached lines (at most 8,192), never the range.
fn missing(
    side: &BTreeMap<u64, String>,
    range: Option<Range<u64>>,
    total: u64,
) -> Option<Range<u64>> {
    let range = range?;
    let range = range.start..range.end.min(total);
    if range.start >= range.end {
        return None;
    }
    let mut first = range.start;
    for (&at, _) in side.range(range.clone()) {
        if at != first {
            break;
        }
        first += 1;
    }
    if first >= range.end {
        return None;
    }
    let mut end = range.end.min(first.saturating_add(MOST));
    for (&at, _) in side.range(first..end).rev() {
        if at + 1 != end {
            break;
        }
        end = at;
    }
    Some(first..end)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A side that holds lines `have`.
    fn side(have: Range<u64>) -> BTreeMap<u64, String> {
        have.map(|at| (at, String::new())).collect()
    }

    #[test]
    fn missing_of_a_range_past_the_total_is_none() {
        let cached = side(0..4);
        assert_eq!(missing(&cached, Some(10..20), 5), None, "start past total");
        assert_eq!(missing(&cached, Some(7..7), 100), None, "empty range");
    }

    #[test]
    fn missing_reads_at_most_one_request_of_a_five_million_line_range() {
        let cached = side(0..KEEP as u64);
        let started = std::time::Instant::now();
        let found = missing(&cached, Some(0..5_000_000), 5_000_000);
        let first = KEEP as u64;
        assert_eq!(found, Some(first..first + MOST), "capped at one request");
        assert!(started.elapsed().as_millis() < 50, "no per-line scan");
        assert_eq!(missing(&cached, Some(0..KEEP as u64), 5_000_000), None);
        let tail = side(first..first + MOST + 5);
        assert_eq!(
            missing(&tail, Some(first..first + MOST + 10), u64::MAX),
            Some(first + MOST + 5..first + MOST + 10),
            "a range with a hole still ends at its last missing line"
        );
    }
}
