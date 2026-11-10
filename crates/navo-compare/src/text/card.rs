//! Card plans for the compare Stream: hunk rows plus a little context, capped.
//!
//! A plan is pure geometry over an `Alignment`; the app turns it into card text.

use std::ops::Range;

use super::diff::{Alignment, Hunk, Row};

const DEFAULT_CONTEXT: usize = 3;
const DEFAULT_MAX_ROWS: usize = 2_000;

/// How much context to show around a hunk and how many card rows may hold it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardLimits {
    /// Unchanged rows shown before and after each hunk.
    pub context: usize,
    /// Maximum card rows, gaps included.
    pub max_rows: usize,
}

impl Default for CardLimits {
    /// Three context lines and 2,000 rows, as D26 sets.
    fn default() -> Self {
        Self {
            context: DEFAULT_CONTEXT,
            max_rows: DEFAULT_MAX_ROWS,
        }
    }
}

/// One row of a card: an aligned row, or a run of unchanged lines left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardRow {
    /// An `Alignment.rows` index and the hunk it belongs to or surrounds.
    Row { index: usize, hunk: usize },
    /// Unchanged rows omitted between two shown blocks.
    Gap { lines: usize },
}

/// The card rows to build, the card-row range of each shown hunk, and the cap counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardPlan {
    /// Rows in display order.
    pub rows: Vec<CardRow>,
    /// For each shown hunk, its range in `rows` (its own rows, not context).
    pub hunks: Vec<Range<usize>>,
    /// Number of hunks that fit under the row cap.
    pub shown_hunks: usize,
    /// Number of hunks left for the "more changes" row.
    pub more_hunks: usize,
}

/// Build the card plan: each hunk with context, merged when close, capped at `limits.max_rows`.
///
/// A hunk that cannot fit whole is not shown; it and the hunks after it are counted in
/// `more_hunks`. The function takes only an `Alignment`, so invalid input fires a debug
/// assertion rather than panicking in a release build.
#[must_use]
pub fn card(alignment: &Alignment, limits: CardLimits) -> CardPlan {
    let own_ranges = hunk_row_ranges(alignment);
    let mut plan = CardPlan::default();
    let mut emitted = 0usize;
    for (index, own) in own_ranges.iter().enumerate() {
        debug_assert!(
            !own.is_empty(),
            "hunk {index} has no aligned rows at line ranges {:?}..{:?}",
            alignment.hunks[index].left_lines,
            alignment.hunks[index].right_lines
        );
        if own.is_empty() {
            break;
        }
        let next = own_ranges
            .get(index + 1)
            .map_or(alignment.rows.len(), |range| range.start);
        let block = Block {
            hunk: index,
            start: own.start.saturating_sub(limits.context).max(emitted),
            own: own.clone(),
            end: own
                .end
                .saturating_add(limits.context)
                .min(next)
                .min(alignment.rows.len()),
        };
        if !push_block(&mut plan, &mut emitted, &block, limits.max_rows) {
            break;
        }
    }
    plan.shown_hunks = plan.hunks.len();
    plan.more_hunks = alignment.hunks.len() - plan.hunks.len();
    plan
}

/// One hunk's rows to show: leading context from `start`, its own rows, trailing context to `end`.
struct Block {
    hunk: usize,
    start: usize,
    own: Range<usize>,
    end: usize,
}

/// Push a block (gap, lead, own rows, trailing context) if the hunk fits under `max_rows`.
///
/// Returns false when the plan is full: the hunk did not fit, or its trailing context was cut.
fn push_block(plan: &mut CardPlan, emitted: &mut usize, block: &Block, max_rows: usize) -> bool {
    let gap = usize::from(block.start > *emitted);
    let needed = plan
        .rows
        .len()
        .saturating_add(gap)
        .saturating_add(block.own.start - block.start)
        .saturating_add(block.own.len());
    if needed > max_rows {
        return false;
    }
    if gap == 1 {
        plan.rows.push(CardRow::Gap {
            lines: block.start - *emitted,
        });
    }
    push_rows(plan, block.start..block.own.start, block.hunk);
    let first = plan.rows.len();
    push_rows(plan, block.own.clone(), block.hunk);
    plan.hunks.push(first..plan.rows.len());
    *emitted = block.own.end;
    let room = max_rows - plan.rows.len();
    let trail = block.end - *emitted;
    push_rows(plan, *emitted..*emitted + trail.min(room), block.hunk);
    *emitted += trail.min(room);
    trail <= room
}

/// Append aligned rows `range` to the plan, all tagged with `hunk`.
fn push_rows(plan: &mut CardPlan, range: Range<usize>, hunk: usize) {
    plan.rows
        .extend(range.map(|index| CardRow::Row { index, hunk }));
}

/// Find the `Alignment.rows` range of each hunk by walking its line ranges in order.
fn hunk_row_ranges(alignment: &Alignment) -> Vec<Range<usize>> {
    let mut ranges = vec![0..0; alignment.hunks.len()];
    let mut hunk_at = 0usize;
    for (at, row) in alignment.rows.iter().enumerate() {
        if matches!(row, Row::Equal { .. }) {
            continue;
        }
        while hunk_at < alignment.hunks.len() && !row_in_hunk(&alignment.hunks[hunk_at], row) {
            hunk_at += 1;
        }
        let Some(range) = ranges.get_mut(hunk_at) else {
            break;
        };
        if range.start == range.end {
            range.start = at;
        }
        range.end = at + 1;
    }
    ranges
}

/// Say whether an aligned row's line indexes fall inside the hunk's ranges.
fn row_in_hunk(hunk: &Hunk, row: &Row) -> bool {
    match *row {
        Row::Equal { .. } => false,
        Row::Changed { left, right } => {
            hunk.left_lines.contains(&left) && hunk.right_lines.contains(&right)
        }
        Row::Deleted { left } => hunk.left_lines.contains(&left),
        Row::Inserted { right } => hunk.right_lines.contains(&right),
    }
}

#[cfg(test)]
mod tests;
