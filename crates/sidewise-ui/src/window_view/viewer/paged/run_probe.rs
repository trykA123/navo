//! Test probe for the highlight runs `text_row` hands to `StyledText::with_highlights`.
//! GPUI test snapshots do not expose runs, so the viewer tests read them here.
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ops::Range;

use gpui_kit::HighlightStyle;

/// How many drawn rows the probe keeps; older rows are dropped first.
const KEEP: usize = 4096;

/// One drawn text row: its start byte, the shown text and its highlight runs.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawnRow {
    pub start: u64,
    pub shown: String,
    pub runs: Vec<(Range<usize>, HighlightStyle)>,
}

thread_local! {
    static ROWS: RefCell<VecDeque<DrawnRow>> = const { RefCell::new(VecDeque::new()) };
}

/// Records one drawn row; keeps the newest 4096 (older rows drop first).
pub fn record(row: DrawnRow) {
    ROWS.with_borrow_mut(|rows| {
        if rows.len() == KEEP {
            rows.pop_front();
        }
        rows.push_back(row);
    });
}

/// Records a drawn row from its parts; the call `text_row` makes before drawing.
pub fn note(start: u64, shown: &str, runs: &[(Range<usize>, HighlightStyle)]) {
    record(DrawnRow {
        start,
        shown: shown.to_string(),
        runs: runs.to_vec(),
    });
}

/// Drains and returns the recorded rows, oldest first.
pub fn take() -> Vec<DrawnRow> {
    ROWS.with_borrow_mut(|rows| rows.drain(..).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(start: u64) -> DrawnRow {
        DrawnRow {
            start,
            shown: format!("row {start}"),
            runs: vec![(0..3, HighlightStyle::default())],
        }
    }

    #[test]
    fn run_probe_returns_records_in_order_then_empty() {
        take();
        record(row(1));
        record(row(2));
        assert_eq!(take(), vec![row(1), row(2)]);
        assert!(take().is_empty());
    }

    #[test]
    fn run_probe_keeps_the_newest_4096() {
        take();
        for start in 0..5000 {
            record(row(start));
        }
        let rows = take();
        assert_eq!(rows.len(), 4096);
        assert_eq!(rows[0].start, 904);
        assert_eq!(rows[4095].start, 4999);
    }
}
