//! Maps between the file diff's list rows and the byte offsets of the left file, for the Alt+V
//! Text / Bytes switch (V41b): the line on top becomes an offset, and an offset becomes a line.

use super::FileView;
use crate::compare_diff::{DiffRow, Layout, PairText, pair_rows, row_count};

/// The left line a diff row shows; an inserted row has none.
fn left_line(row: DiffRow) -> Option<usize> {
    match row {
        DiffRow::Equal(left, _) | DiffRow::Changed(left, _) | DiffRow::Deleted(left) => {
            Some(left as usize)
        }
        DiffRow::Inserted(_) => None,
    }
}

/// How many list rows one scan step reads.
const STEP: usize = 4096;

/// The first list row at or after `from` whose left line passes `stop`, with that line; the
/// walk reads the real rows of `layout`, so interleaved and run-ordered lists both map right.
fn scan_left(
    text: &PairText,
    layout: Layout,
    from: usize,
    stop: impl Fn(usize) -> bool,
) -> Option<(usize, usize)> {
    let mut start = from;
    loop {
        let shown = pair_rows(text, layout, start..start.saturating_add(STEP));
        if shown.is_empty() {
            return None;
        }
        let found = shown.iter().enumerate().find_map(|(at, row)| {
            let (line, _) = row.left?;
            stop(line as usize).then_some((start + at, line as usize))
        });
        if found.is_some() {
            return found;
        }
        start = start.saturating_add(shown.len());
    }
}

/// The left line list `row` shows, or the next one when the row has only a right side; the
/// line count when no left line follows.
fn line_at_row(text: &PairText, layout: Layout, row: usize) -> usize {
    scan_left(text, layout, row, |_| true).map_or(text.left.len(), |(_, line)| line)
}

/// The first list row that shows left line `line` or a later one; the last row when none does.
fn row_of_line(text: &PairText, layout: Layout, line: usize) -> usize {
    // A list row is never ahead of its `DiffRow` number, so the scan starts there.
    let at = text
        .rows
        .iter()
        .position(|row| left_line(*row).is_some_and(|shown| shown >= line))
        .unwrap_or(text.rows.len().saturating_sub(1));
    scan_left(text, layout, at, |shown| shown >= line)
        .map_or_else(|| row_count(text, layout).saturating_sub(1), |(row, _)| row)
}

/// The byte offset where left line `line` starts; the file's end past the last line, 0 when
/// the pair carries no offsets.
fn offset_of_line(text: &PairText, line: usize) -> u64 {
    let starts = &text.left_starts;
    starts
        .get(line)
        .or_else(|| starts.last())
        .copied()
        .unwrap_or(0)
}

/// The left line that holds byte `offset`; the last line past the end.
fn line_of_offset(text: &PairText, offset: u64) -> usize {
    let passed = text.left_starts.partition_point(|&start| start <= offset);
    passed
        .saturating_sub(1)
        .min(text.left.len().saturating_sub(1))
}

impl FileView {
    /// The byte offset in the left file of the line on top of the list, rows `row_height` tall.
    pub(in crate::window_view::compare_screen) fn top_offset(
        &self,
        layout: Layout,
        row_height: gpui_kit::Pixels,
    ) -> u64 {
        let row = self.scroll.top_row(row_height);
        offset_of_line(&self.text, line_at_row(&self.text, layout, row))
    }

    /// Puts the line that holds byte `offset` of the left file on top at the next draw.
    pub(in crate::window_view::compare_screen) fn seek_offset(&mut self, offset: u64) {
        self.seek = Some(line_of_offset(&self.text, offset));
    }

    /// Scrolls to a pending seek in `layout`, once.
    pub(super) fn apply_seek(&mut self, layout: Layout) {
        if let Some(line) = self.seek.take() {
            self.scroll
                .scroll_to_top(row_of_line(&self.text, layout, line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{line_at_row, line_of_offset, offset_of_line, row_of_line};
    use crate::compare_diff::{DiffRow, HunkView, Layout, PairFacts, PairText, ViewedKey};

    /// Five lines of ten bytes; the right side inserts a row after left line 1 and changes line 3.
    fn text() -> PairText {
        PairText {
            left: vec![String::new(); 5],
            right: vec![String::new(); 6],
            left_starts: vec![0, 10, 20, 30, 40, 50],
            rows: vec![
                DiffRow::Equal(0, 0),
                DiffRow::Equal(1, 1),
                DiffRow::Inserted(2),
                DiffRow::Equal(2, 3),
                DiffRow::Changed(3, 4),
                DiffRow::Equal(4, 5),
            ],
            hunks: [2..3, 4..5]
                .into_iter()
                .map(|rows| HunkView {
                    rows,
                    left_bytes: 0..0,
                    right_bytes: 0..0,
                    digest: None,
                    added: 1,
                    removed: 1,
                })
                .collect(),
            facts: PairFacts {
                left_encoding: "",
                right_encoding: "",
                left_endings: "",
                right_endings: "",
                raw_equal: false,
                display_equal: false,
                fallbacks: Vec::new(),
            },
            viewed: ViewedKey([0; 64]),
        }
    }

    #[test]
    fn a_row_maps_to_its_left_line_or_the_next_one() {
        let text = text();
        assert_eq!(line_at_row(&text, Layout::SideBySide, 1), 1);
        assert_eq!(
            line_at_row(&text, Layout::SideBySide, 2),
            2,
            "inserted: next line"
        );
        assert_eq!(line_at_row(&text, Layout::SideBySide, 9), 5, "past the end");
        assert_eq!(
            line_at_row(&text, Layout::Unified, 5),
            4,
            "the changed row, inserted half: the next left line"
        );
    }

    #[test]
    fn a_line_maps_to_its_list_row_in_both_layouts() {
        let text = text();
        assert_eq!(row_of_line(&text, Layout::SideBySide, 2), 3);
        assert_eq!(row_of_line(&text, Layout::SideBySide, 4), 5);
        assert_eq!(
            row_of_line(&text, Layout::Unified, 4),
            6,
            "the changed row takes two"
        );
        assert_eq!(
            row_of_line(&text, Layout::SideBySide, 99),
            5,
            "past the end: last row"
        );
    }

    #[test]
    fn offsets_and_lines_round_trip_through_the_line_starts() {
        let text = text();
        assert_eq!(offset_of_line(&text, 3), 30);
        assert_eq!(offset_of_line(&text, 99), 50, "past the last line: the end");
        assert_eq!(line_of_offset(&text, 0), 0);
        assert_eq!(line_of_offset(&text, 29), 2, "inside a line");
        assert_eq!(line_of_offset(&text, 30), 3);
        assert_eq!(line_of_offset(&text, 999), 4, "past the end: last line");
    }
}
