//! The rows of the file diff list: what one frame needs to draw a range of list rows, and the
//! wrap that adds the copy bar and the right-click menu to each.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use super::super::context::{RowHit, on_right_click};
use super::super::copy::FileBars;
use super::sideways::{self, SidewaysHandle};
use super::wrapped::WrapDraw;
use gpui_kit::component::highlighter::HighlightTheme;

use crate::compare_diff::body::{BodyLook, PairSyntax, SyntaxLook, row_view_cut};
use crate::compare_diff::{Layout, OnBytesHere, PairText, ShownRow, SpanCache, Ticks, pair_rows};
use crate::prelude::*;
use crate::tokens;

/// Everything the list's rows closure holds between frames.
pub(super) struct ListRows {
    pub(super) text: Arc<PairText>,
    /// The rows built in the last frame, written for the change map and the span requests.
    pub(super) built: Rc<Cell<(usize, usize)>>,
    /// The first pair row of every change.
    pub(super) starts: Vec<usize>,
    pub(super) layout: Layout,
    pub(super) current: Option<u32>,
    pub(super) digits: usize,
    pub(super) spans: Rc<RefCell<SpanCache>>,
    pub(super) words: bool,
    pub(super) bars: FileBars,
    pub(super) on_long: OnBytesHere,
    /// The sideways offset every row's text is cut by; the first row carries its probe.
    pub(super) sideways: SidewaysHandle,
    /// The wrap index and width while lines wrap; `None` draws one item per pair row.
    pub(super) wrap: Option<WrapDraw>,
    /// The sides' finished syntax parse, if any (D38).
    pub(super) syntax: Option<Rc<PairSyntax>>,
    /// The theme's highlight colours, as F3 reads them.
    pub(super) colours: Option<Arc<HighlightTheme>>,
}

impl ListRows {
    /// Draws list items `range` (pair rows, or visual lines while wrapping), cut at this frame's
    /// sideways offset or wrap width, and records the pair rows they show as built.
    pub(super) fn draw(
        &self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let (text, picks, bars) = (&self.text, &self.bars.picks, &self.bars);
        let (rows, placed) = self.place(range.clone());
        self.built.set((rows.start, rows.end));
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
            syntax: SyntaxLook::new(self.syntax.as_deref(), self.colours.as_deref()),
        };
        let cut = self.sideways.frame(sideways::advance(window, look.theme));
        let shown = pair_rows(text, self.layout, rows.clone());
        let placed = placed.into_iter().zip(range).enumerate();
        placed
            .filter_map(|(i, ((at, sub), item))| {
                let row = shown.get(at - rows.start)?;
                let mut cut = self
                    .wrap
                    .as_ref()
                    .map_or_else(|| cut.clone(), |w| w.cut(sub));
                if i == 0 {
                    cut.probe = Some(self.sideways.probe());
                }
                let drawn = row_view_cut(&look, row, &cut);
                let first =
                    sub == 0 && row.hunk.and_then(|h| self.starts.get(h as usize)) == Some(&at);
                Some(wrap_row(bars, look.theme, drawn, (item, first, row, text)))
            })
            .collect()
    }

    /// The pair rows list items `items` show, and each item's pair row and visual line.
    fn place(&self, items: Range<usize>) -> (Range<usize>, Vec<(usize, u32)>) {
        match &self.wrap {
            Some(wrap) => wrap.place(items),
            None => (items.clone(), items.map(|row| (row, 0)).collect()),
        }
    }
}

/// Wraps list item `id` (drawn from `row`) with the copy bar when it belongs to a copyable change
/// of `text`, and with the right-click menu; the bar's buttons sit on the change's `first` item.
fn wrap_row(
    bars: &FileBars,
    t: &tokens::Theme,
    drawn: AnyElement,
    (id, first, row, text): (usize, bool, &ShownRow, &PairText),
) -> AnyElement {
    let hunk = row
        .hunk
        .filter(|&h| text.hunks[h as usize].digest.is_some());
    let shown = match hunk {
        Some(hunk) => bars.wrap(t, drawn, (id, hunk, first)),
        None => drawn,
    };
    on_right_click(&bars.weak, shown, RowHit::of(row))
}
