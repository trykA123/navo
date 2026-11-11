//! The rows of the file diff list: what one frame needs to draw a range of list rows, and the
//! wrap that adds the copy bar and the right-click menu to each.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use super::super::context::{RowHit, on_right_click};
use super::super::copy::FileBars;
use crate::compare_diff::body::{BodyLook, row_view};
use crate::compare_diff::{Layout, OnBytesHere, PairText, ShownRow, SpanCache, Ticks, pair_rows};
use crate::prelude::*;
use crate::tokens;

/// Everything the list's rows closure holds between frames.
pub(super) struct ListRows {
    pub(super) text: Arc<PairText>,
    /// The rows built in the last frame, written for the change map and the span requests.
    pub(super) built: Rc<Cell<(usize, usize)>>,
    /// The first list row of every change.
    pub(super) starts: Vec<usize>,
    pub(super) layout: Layout,
    pub(super) current: Option<u32>,
    pub(super) digits: usize,
    pub(super) spans: Rc<RefCell<SpanCache>>,
    pub(super) words: bool,
    pub(super) bars: FileBars,
    pub(super) on_long: OnBytesHere,
}

impl ListRows {
    /// Draws list rows `range` and records them as built.
    pub(super) fn draw(&self, range: Range<usize>, cx: &mut App) -> Vec<AnyElement> {
        let (text, picks, bars) = (&self.text, &self.bars.picks, &self.bars);
        self.built.set((range.start, range.end));
        let cache = self.words.then(|| self.spans.borrow());
        let ticks = Ticks {
            text,
            side: picks.side,
            picked: &picks.lines,
            cursor: picks.cursor,
            hovered: bars.hovered,
            on_tick: Rc::clone(&picks.on_tick),
        };
        let look = BodyLook {
            theme: tokens::current(cx),
            layout: self.layout,
            digits: self.digits,
            current: self.current,
            spans: cache.as_deref(),
            ticks: Some(&ticks),
            bytes_here: Some(&self.on_long),
        };
        let first = range.start;
        let rows = pair_rows(text, self.layout, range);
        rows.iter()
            .enumerate()
            .map(|(i, row)| {
                let drawn = row_view(&look, row);
                wrap_row(bars, look.theme, drawn, (first + i, row, (text, &self.starts)))
            })
            .collect()
    }
}

/// Wraps list row `at` (shown as `row`) with the copy bar when it belongs to a copyable change
/// of `text`, and with the right-click menu; `starts` holds the first list row of every change.
fn wrap_row(
    bars: &FileBars,
    t: &tokens::Theme,
    drawn: AnyElement,
    (at, row, (text, starts)): (usize, &ShownRow, (&PairText, &[usize])),
) -> AnyElement {
    let hunk = row
        .hunk
        .filter(|&h| text.hunks[h as usize].digest.is_some());
    let shown = match hunk {
        Some(hunk) => {
            let first_row = starts.get(hunk as usize) == Some(&at);
            bars.wrap(t, drawn, (at, hunk, first_row))
        }
        None => drawn,
    };
    on_right_click(&bars.weak, shown, RowHit::of(row))
}
