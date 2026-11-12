//! The file diff body: a virtual list of the pair's rows for the viewport, the current change,
//! and next and previous change (Alt+↓/↑, F7/Shift+F7), each centred in the list.

mod map;
mod reveal;
mod rows;
mod seek;
mod sideways;
mod spans;
mod strip;
mod syntax;
mod widths;
mod wrapped;

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::Task;
use navo_l10n::{t, tr};

use super::super::layout::ONE_PANE_BELOW;
use super::copy::FileBars;
use super::switch;
use crate::compare_diff::body::digits;
use crate::compare_diff::{DiffRow, Layout, LineCols, PairText, SpanCache, row_count};
use crate::prelude::*;
use crate::primitives::{ListScroll, VirtualList};
use crate::tokens;
use crate::window_view::ScreenOwner;
use sideways::SidewaysHandle;
#[cfg(test)]
pub(in crate::window_view) use sideways::SidewaysSeen;

/// How a pair lays out in one layout: its row count and the first row of each hunk.
struct Shape {
    count: usize,
    starts: Vec<usize>,
}

impl Shape {
    /// Measures `text` in `layout`. Unified shows a changed row as two rows (deleted, then
    /// inserted), so each hunk starts later by the changed rows of the hunks before it.
    fn new(text: &PairText, layout: Layout) -> Self {
        let mut extra = 0;
        let starts = text
            .hunks
            .iter()
            .map(|hunk| {
                let start = hunk.rows.start as usize + extra;
                if layout == Layout::Unified {
                    let rows = text
                        .rows
                        .get(hunk.rows.start as usize..hunk.rows.end as usize);
                    let changed = rows.unwrap_or_default().iter();
                    extra += changed
                        .filter(|row| matches!(row, DiffRow::Changed(..)))
                        .count();
                }
                start
            })
            .collect();
        Self {
            count: row_count(text, layout),
            starts,
        }
    }
}

/// The layout for a window: side by side, or unified below 900 px.
pub(super) fn layout_for(window: &Window) -> Layout {
    match window.viewport_size().width < ONE_PANE_BELOW {
        true => Layout::Unified,
        false => Layout::SideBySide,
    }
}

/// An opened text pair as the file diff shows it.
pub(super) struct FileView {
    text: Arc<PairText>,
    /// Shapes per layout, measured once per text: unified counting walks every hunk row.
    shapes: [Option<Shape>; 2],
    current: Option<usize>,
    /// Whether the list still has to centre the current change on its next draw: set when a pair
    /// opens on a change and by every jump to one, and kept while wrap waits for its index.
    centre: bool,
    scroll: ListScroll,
    /// The rows the list built in its last frame.
    built: Rc<Cell<(usize, usize)>>,
    /// The change map, mounted on the first frame that draws it.
    map: Option<map::MapMount>,
    /// Changed spans inside lines, filled for the rows asked about.
    spans: Rc<RefCell<SpanCache>>,
    /// Whether changed words show: only then spans are asked for and painted.
    words: bool,
    /// The rows built when the wait for the next span request began.
    watched: (usize, usize),
    /// The wait before spans are asked for; dropping it cancels the wait.
    settle: Option<Task<()>>,
    /// Whether the extra first frame was asked for.
    retried: bool,
    /// The left line to put on top at the next draw (Alt+V back from bytes).
    seek: Option<usize>,
    /// The sideways scroll both text columns share.
    sideways: SidewaysHandle,
    /// The column count of every line, once the background measure landed.
    cols: Option<Arc<LineCols>>,
    /// The background measure of `cols`, started by the window once the text is open.
    widths: widths::WidthsSlot,
    /// The sides' syntax parse (D38), started by the window once the text is open.
    syntax: syntax::SyntaxSlot,
    /// Wrapped lines and their index (D39.5–D39.6).
    wrapped: wrapped::Wrapped,
}

impl FileView {
    /// A view of `text` with change `at` current and centred on the first draw; with no `at`,
    /// or one past the last change, it starts at the top with no current change.
    /// `words` paints changed words.
    pub(super) fn new(text: Arc<PairText>, at: Option<usize>, words: bool) -> Self {
        let current = at.filter(|&at| at < text.hunks.len());
        Self {
            text,
            shapes: [None, None],
            current,
            centre: current.is_some(),
            scroll: ListScroll::new(),
            built: Rc::default(),
            map: None,
            spans: Rc::default(),
            words,
            watched: (0, 0),
            settle: None,
            retried: false,
            seek: None,
            sideways: SidewaysHandle::default(),
            cols: None,
            widths: widths::WidthsSlot::default(),
            syntax: syntax::SyntaxSlot::default(),
            wrapped: wrapped::Wrapped::default(),
        }
    }

    /// Keeps the sideways offset of `old`, the same pair before a reload.
    pub(super) fn keep_sideways(&mut self, old: &Self) {
        self.sideways = SidewaysHandle::carried(&old.sideways);
    }

    /// Keeps the syntax parse of `old`, the same pair before a reload, to colour the rows with
    /// until this text's own parse lands.
    pub(super) fn keep_syntax(&mut self, old: &Self) {
        self.syntax.keep_from(&old.syntax);
    }

    /// The shape in `layout`, measured on first use.
    fn shape(&mut self, layout: Layout) -> &Shape {
        let slot = &mut self.shapes[usize::from(layout == Layout::Unified)];
        slot.get_or_insert_with(|| Shape::new(&self.text, layout))
    }

    /// Moves to the next (`forward`) or previous change and centres it; stops at the ends.
    /// From no current change, forward goes to the first and back to the last.
    fn step(&mut self, forward: bool, layout: Layout) -> bool {
        let last = match self.text.hunks.len() {
            0 => return false,
            len => len - 1,
        };
        let next = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => last,
            (Some(at), true) => (at + 1).min(last),
            (Some(at), false) => at.saturating_sub(1),
        };
        self.go_to(next, layout);
        true
    }

    /// Makes change `hunk` the current one and centres its first list item: now through the index
    /// the view has, and again on the next draw, so a wrap index rebuilt in that frame (`layout`
    /// measures the pair then) still ends with the change in the middle.
    pub(super) fn go_to(&mut self, hunk: usize, layout: Layout) {
        self.current = Some(hunk);
        self.centre = true;
        if let Some(&row) = self.shape(layout).starts.get(hunk) {
            self.scroll.scroll_to_center(self.list_row(row));
        }
    }

    /// Makes change `hunk` the current one without scrolling (a right-click put the cursor there).
    pub(super) fn set_current(&mut self, hunk: usize) {
        if hunk < self.text.hunks.len() {
            self.current = Some(hunk);
        }
    }

    /// The current change, if any.
    pub(super) fn current(&self) -> Option<usize> {
        self.current
    }

    /// How many changes the pair has.
    pub(super) fn change_count(&self) -> usize {
        self.text.hunks.len()
    }

    /// The pair's text, for the drawers.
    pub(super) fn text(&self) -> &Arc<PairText> {
        &self.text
    }

    /// The status line text: "Change i of n", "n changes" or "No changes".
    pub(super) fn status(&self) -> SharedString {
        let total = self.text.hunks.len();
        let text = match (self.current, total) {
            (_, 0) => tr("compare-screen-no-changes"),
            (Some(at), _) => t!("compare-screen-change", current = at + 1, total = total),
            (None, _) => t!("compare-screen-changes", total = total),
        };
        text.into()
    }

    /// The row list for the viewport in `layout` above the sideways strip; rows of a copyable
    /// change report hover and carry the copy bar when `bars` is given.
    pub(super) fn list(&mut self, layout: Layout, bars: FileBars, cx: &App) -> AnyElement {
        let row_height = tokens::current(cx).spacing.row_height;
        // The index first: the centre and the seek that follow read it, and its re-anchor stood
        // down for a pending centre.
        self.index(layout, row_height);
        self.place(layout);
        let (count, starts) = {
            let shape = self.shape(layout);
            (shape.count, shape.starts.clone())
        };
        let count = self.items(count);
        let (syntax, colours) = self.syntax.parts(cx);
        let rows = rows::ListRows {
            text: Arc::clone(&self.text),
            built: Rc::clone(&self.built),
            starts,
            layout,
            current: self.current.and_then(|at| u32::try_from(at).ok()),
            digits: digits(&self.text),
            spans: Rc::clone(&self.spans),
            words: self.words,
            on_long: switch::on_bytes_here(&bars.weak),
            sideways: self.sideways.clone(),
            wrap: self.wrap_draw(),
            syntax,
            colours,
            bars,
        };
        let on_wheel = sideways::on_wheel(self.sideways.clone(), rows.bars.weak.clone());
        let rows = move |range: Range<usize>, window: &mut Window, cx: &mut App| {
            rows.draw(range, window, cx)
        };
        let list = VirtualList::new("compare-file-rows", count, &self.scroll, rows)
            .row_height(row_height)
            .label(tr("compare-screen-rows"))
            .axis_locked();
        div()
            .flex()
            .flex_col()
            .size_full()
            .on_scroll(on_wheel)
            .child(list)
            .child(strip::strip(&self.sideways, layout))
            .into_any_element()
    }
}

impl ScreenOwner {
    /// Next or previous change in the open file diff.
    pub(super) fn step_change(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut view = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        if let Some(hex) = view.as_mut().and_then(|file| file.hex.as_mut()) {
            let visible = super::hex::visible_rows(tokens::current(cx), window);
            if hex.step(forward, visible) {
                self.compare.diff.copy.note = None;
                cx.notify();
            }
            return;
        }
        let layout = view
            .as_ref()
            .map_or_else(|| layout_for(window), |file| file.look.layout_in(window));
        let moved = view.is_some_and(|file| match (file.view.as_mut(), file.large.as_mut()) {
            (Some(view), _) => view.step(forward, layout),
            (None, Some(large)) => large.step(forward),
            (None, None) => false,
        });
        if moved {
            self.compare.diff.copy.note = None;
            cx.notify();
        }
    }
}

#[cfg(test)]
impl FileView {
    /// The current change, the rows built in the last frame and the row count in `layout`.
    pub(super) fn probe(&mut self, layout: Layout) -> (Option<usize>, Range<usize>, usize) {
        let (start, end) = self.built.get();
        (self.current, start..end, self.shape(layout).count)
    }

    /// The sideways offset, cut and extent.
    pub(super) fn sideways_seen(&self) -> SidewaysSeen {
        self.sideways.seen()
    }
}

#[cfg(test)]
mod tests;
