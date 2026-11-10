//! Narrows a card to the hunks a search matched (D30): the others fold into one note row.

use super::super::hooks::CardRow;

/// What a narrowed card draws, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    /// The card row with this index.
    Row(usize),
    /// A run of this many hunks that did not match, folded into one row.
    Others(u32),
}

/// The slots of `rows` when only the hunks in `keep` stay; every row when `keep` is `None`.
/// A gap stays only between two kept hunks; a run of skipped hunks becomes one `Others`.
pub(super) fn slots(rows: &[CardRow], keep: Option<&[u32]>) -> Vec<Slot> {
    let Some(keep) = keep else {
        return (0..rows.len()).map(Slot::Row).collect();
    };
    let mut run = Run::default();
    let mut out = Vec::new();
    for (at, row) in rows.iter().enumerate() {
        match row {
            CardRow::Line { hunk, .. } if keep.contains(hunk) => {
                run.flush(&mut out);
                out.extend(run.gap.take().map(Slot::Row));
                out.push(Slot::Row(at));
                run.after_kept = true;
            }
            CardRow::Line { hunk, .. } => run.skip(*hunk),
            CardRow::Gap { .. } if run.after_kept => run.gap = Some(at),
            CardRow::Gap { .. } => {}
            CardRow::More { .. } => {
                run.flush(&mut out);
                out.push(Slot::Row(at));
                run.after_kept = false;
            }
        }
    }
    run.flush(&mut out);
    out
}

/// The skipped hunks since the last kept row, and the gap that may precede the next kept one.
#[derive(Default)]
struct Run {
    skipped: u32,
    last: Option<u32>,
    gap: Option<usize>,
    after_kept: bool,
}

impl Run {
    /// Notes a line of a skipped hunk; the run counts each hunk once.
    fn skip(&mut self, hunk: u32) {
        if self.last != Some(hunk) {
            self.skipped += 1;
            self.last = Some(hunk);
        }
        self.gap = None;
        self.after_kept = false;
    }

    /// Ends the run: pushes one `Others` for the skipped hunks, if any.
    fn flush(&mut self, out: &mut Vec<Slot>) {
        if self.skipped > 0 {
            out.push(Slot::Others(self.skipped));
        }
        self.skipped = 0;
        self.last = None;
    }
}

#[cfg(test)]
mod tests;
