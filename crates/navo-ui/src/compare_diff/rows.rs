//! Row builders for the file diff and the Stream: what to draw for a range of rows.
//! Pure and cheap: `ShownRow`s borrow the text and are built per frame for the visible range.

use std::ops::Range;

use super::hooks::{CardRow, DiffRow, Layout, PairText};
use super::stream_model::StreamModel;
use narrow::Slot;

mod narrow;
mod unified;

/// How a shown row looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShownKind {
    Equal,
    Changed,
    Deleted,
    Inserted,
    Filler,
    /// Lines the large diff has not settled yet (still diffing, or an evicted section).
    Pending,
}

/// One drawn row. Each side holds a line index and its text, or nothing (a filler side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShownRow<'a> {
    pub kind: ShownKind,
    pub left: Option<(u32, &'a str)>,
    pub right: Option<(u32, &'a str)>,
    pub hunk: Option<u32>,
    /// The index of the `DiffRow` this row draws (a side of it, in unified).
    pub row: u32,
}

/// Which side of a card line a Stream row draws. Side by side draws `Both`; unified draws a
/// changed line as a `Left` (deleted) row, or a `Right` (inserted) row after the run's lefts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    Both,
    Left,
    Right,
}

/// One row of the Stream list; `usize` fields are item indexes into the model's items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRow {
    Header(usize),
    Line {
        item: usize,
        card_row: u32,
        half: Half,
    },
    Gap {
        item: usize,
        lines: u32,
    },
    More {
        item: usize,
        hunks: u32,
    },
    /// Hunks a search did not match, folded into one row.
    Others {
        item: usize,
        hunks: u32,
    },
    Placeholder(usize),
    Summary(usize),
    RulesGroup,
}

/// How many rows the pair draws in `layout`. Unified walks the hunks' rows, so the host
/// keeps the result while the text stays the same.
#[must_use]
pub fn row_count(text: &PairText, layout: Layout) -> usize {
    match layout {
        Layout::SideBySide => text.rows.len(),
        Layout::Unified => unified::count(text),
    }
}

/// Builds the rows in `range` (clamped to the pair). Side by side is one row per `DiffRow`,
/// a lone line facing a filler; unified lists a hunk's deleted lines before its inserted ones.
#[must_use]
pub fn pair_rows(text: &PairText, layout: Layout, range: Range<usize>) -> Vec<ShownRow<'_>> {
    match layout {
        Layout::SideBySide => {
            let end = range.end.min(text.rows.len());
            let start = range.start.min(end);
            (start..end)
                .map(|at| side_row(text, at, hunk_of(text, at)))
                .collect()
        }
        Layout::Unified => unified::rows(text, range),
    }
}

/// The hunk that holds `DiffRow` number `at`, found by binary search over the sorted hunks.
fn hunk_of(text: &PairText, at: usize) -> Option<u32> {
    let at = u32::try_from(at).ok()?;
    let found = text.hunks.partition_point(|h| h.rows.end <= at);
    text.hunks
        .get(found)
        .filter(|h| h.rows.start <= at)
        .and_then(|_| u32::try_from(found).ok())
}

/// Line `index` of one side with its text; `None` when the index is past the side's end.
fn line(lines: &[String], index: u32) -> Option<(u32, &str)> {
    lines
        .get(usize::try_from(index).ok()?)
        .map(|s| (index, s.as_str()))
}

/// One side-by-side row for `text.rows[at]`; a lone line faces a filler.
fn side_row(text: &PairText, at: usize, hunk: Option<u32>) -> ShownRow<'_> {
    let (kind, left, right) = match text.rows[at] {
        DiffRow::Equal(l, r) => (ShownKind::Equal, Some(l), Some(r)),
        DiffRow::Changed(l, r) => (ShownKind::Changed, Some(l), Some(r)),
        DiffRow::Deleted(l) => (ShownKind::Deleted, Some(l), None),
        DiffRow::Inserted(r) => (ShownKind::Inserted, None, Some(r)),
    };
    ShownRow {
        kind,
        left: left.and_then(|i| line(&text.left, i)),
        right: right.and_then(|i| line(&text.right, i)),
        hunk,
        row: u32::try_from(at).unwrap_or(u32::MAX),
    }
}

/// The Stream's list: per shown item a header, then its card rows when the card is open.
/// Folded and viewed cards give a header only; an unbuilt card a placeholder; a card without
/// rows (large, binary, image, unavailable) a summary. Cards follow `layout`: in unified a
/// hunk lists its deleted lines, then its inserted lines, as `pair_rows` does.
#[must_use]
pub fn stream_rows(model: &StreamModel, layout: Layout) -> Vec<StreamRow> {
    let mut out = Vec::new();
    for item in model.visible() {
        out.push(StreamRow::Header(item));
        let id = model.items[item].id;
        if !model.is_open(id) {
            continue;
        }
        let Some(card) = model.card(id) else {
            out.push(StreamRow::Placeholder(item));
            continue;
        };
        if card.rows.is_empty() {
            out.push(StreamRow::Summary(item));
        }
        let slots = narrow::slots(&card.rows, model.matched_hunks(id));
        card_rows(&mut out, item, &card.rows, (&slots, layout));
    }
    if model.rules > 0 && !model.searching() {
        out.push(StreamRow::RulesGroup);
    }
    out
}

/// Appends the Stream rows of one card's `slots`. Row numbers past `u32::MAX` are dropped.
fn card_rows(out: &mut Vec<StreamRow>, item: usize, rows: &[CardRow], plan: (&[Slot], Layout)) {
    let (slots, layout) = plan;
    let mut next = 0;
    while let Some(&slot) = slots.get(next) {
        next += 1;
        let at = match slot {
            Slot::Row(at) => at,
            Slot::Others(hunks) => {
                out.push(StreamRow::Others { item, hunks });
                continue;
            }
        };
        let Ok(card_row) = u32::try_from(at) else {
            return;
        };
        match &rows[at] {
            CardRow::Line { row, .. } if layout == Layout::Unified && changed(row) => {
                let end = run_end(rows, at);
                unified_run(out, item, rows, at..end);
                // The run's other rows are drawn now; skip their slots.
                next += slots[next..]
                    .iter()
                    .take_while(|slot| matches!(slot, Slot::Row(i) if *i < end))
                    .count();
            }
            CardRow::Line { .. } => out.push(StreamRow::Line {
                item,
                card_row,
                half: Half::Both,
            }),
            CardRow::Gap { lines } => out.push(StreamRow::Gap {
                item,
                lines: *lines,
            }),
            CardRow::More { hunks } => out.push(StreamRow::More {
                item,
                hunks: *hunks,
            }),
        }
    }
}

/// Whether a card line is part of a change (not an equal context line).
fn changed(row: &DiffRow) -> bool {
    !matches!(row, DiffRow::Equal(..))
}

/// The end of the run of changed lines of one hunk that starts at `start`.
fn run_end(rows: &[CardRow], start: usize) -> usize {
    let hunk = |row: &CardRow| match row {
        CardRow::Line { hunk, row, .. } if changed(row) => Some(*hunk),
        _ => None,
    };
    let first = hunk(&rows[start]);
    (start..rows.len())
        .find(|&at| hunk(&rows[at]) != first)
        .unwrap_or(rows.len())
}

/// Picks one side's line index from a diff row.
type SideOf = fn(DiffRow) -> Option<u32>;

/// One unified run: every left line (deleted) first, then every right line (inserted).
fn unified_run(out: &mut Vec<StreamRow>, item: usize, rows: &[CardRow], run: Range<usize>) {
    let sides: [(Half, SideOf); 2] = [(Half::Left, unified::left), (Half::Right, unified::right)];
    for (half, side) in sides {
        for at in run.clone() {
            let has = matches!(&rows[at], CardRow::Line { row, .. } if side(*row).is_some());
            if let (true, Ok(card_row)) = (has, u32::try_from(at)) {
                out.push(StreamRow::Line {
                    item,
                    card_row,
                    half,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
