//! The file diff body: a virtual list of the pair's rows for the viewport, the current change,
//! and next and previous change (Alt+↓/↑, F7/Shift+F7), each centred in the list.

mod map;
mod spans;

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::Task;
use navo_l10n::{t, tr};

use super::super::layout::ONE_PANE_BELOW;
use crate::compare_diff::body::{BodyLook, digits, row_view};
use crate::compare_diff::{DiffRow, Layout, PairText, SpanCache, pair_rows, row_count};
use crate::prelude::*;
use crate::primitives::{ListScroll, VirtualList};
use crate::tokens;
use crate::window_view::NavoWindow;

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
    /// Whether the list still has to centre the current change on its next draw.
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
        }
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

    /// Makes change `hunk` the current one and centres it.
    pub(super) fn go_to(&mut self, hunk: usize, layout: Layout) {
        self.current = Some(hunk);
        if let Some(&row) = self.shape(layout).starts.get(hunk) {
            self.scroll.scroll_to_center(row);
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

    /// The row list for the viewport in `layout`.
    pub(super) fn list(&mut self, layout: Layout, cx: &App) -> AnyElement {
        if std::mem::take(&mut self.centre)
            && let Some(at) = self.current
        {
            let row = self.shape(layout).starts[at];
            self.scroll.scroll_to_center(row);
        }
        let count = self.shape(layout).count;
        let (text, built) = (Arc::clone(&self.text), Rc::clone(&self.built));
        let current = self.current.and_then(|at| u32::try_from(at).ok());
        let digits = digits(&text);
        let (spans, words) = (Rc::clone(&self.spans), self.words);
        let rows = move |range: Range<usize>, _: &mut Window, cx: &mut App| {
            built.set((range.start, range.end));
            let cache = words.then(|| spans.borrow());
            let look = BodyLook {
                theme: tokens::current(cx),
                layout,
                digits,
                current,
                spans: cache.as_deref(),
            };
            pair_rows(&text, layout, range)
                .iter()
                .map(|row| row_view(&look, row))
                .collect()
        };
        VirtualList::new("compare-file-rows", count, &self.scroll, rows)
            .row_height(tokens::current(cx).spacing.row_height)
            .label(tr("compare-screen-rows"))
            .into_any_element()
    }
}

impl NavoWindow {
    /// Next or previous change in the open file diff.
    pub(super) fn step_change(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = self.compare.screen.as_mut().and_then(|s| s.file.as_mut());
        let layout = view
            .as_ref()
            .map_or_else(|| layout_for(window), |file| file.look.layout_in(window));
        if view
            .and_then(|file| file.view.as_mut())
            .is_some_and(|view| view.step(forward, layout))
        {
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
}

#[cfg(test)]
mod tests {
    use super::Shape;
    use crate::compare_diff::{DiffRow, HunkView, Layout, PairFacts, PairText, ViewedKey};

    /// Hunk `rows` with no bytes or digest.
    fn hunk(rows: std::ops::Range<u32>) -> HunkView {
        HunkView {
            rows,
            left_bytes: 0..0,
            right_bytes: 0..0,
            digest: None,
            added: 0,
            removed: 0,
        }
    }

    #[test]
    fn compare_screen_unified_hunks_start_after_the_rows_changed_before_them() {
        let rows = vec![
            DiffRow::Equal(0, 0),
            DiffRow::Changed(1, 1),
            DiffRow::Changed(2, 2),
            DiffRow::Equal(3, 3),
            DiffRow::Deleted(4),
            DiffRow::Changed(5, 4),
        ];
        let text = PairText {
            left: vec![String::new(); 6],
            right: vec![String::new(); 5],
            rows,
            hunks: vec![hunk(1..3), hunk(4..6)],
            facts: PairFacts {
                left_encoding: "",
                right_encoding: "",
                left_endings: "",
                right_endings: "",
                raw_equal: false,
                display_equal: false,
                fallbacks: vec![],
            },
            viewed: ViewedKey([0; 64]),
        };
        let side = Shape::new(&text, Layout::SideBySide);
        assert_eq!((side.count, side.starts), (6, vec![1, 4]));
        let unified = Shape::new(&text, Layout::Unified);
        assert_eq!((unified.count, unified.starts), (9, vec![1, 6]));
    }
}
