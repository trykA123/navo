//! Asks the engine for the changed spans of the rows on screen, once a scroll has settled,
//! and keeps what comes back. Only the look with changed words asks and paints (D58).

use std::ops::Range;
use std::time::Duration;

use super::FileView;
use crate::compare_diff::{Layout, RowSpans, pair_rows};
use crate::prelude::*;
use crate::window_view::ScreenOwner;

/// How long the visible rows must stay the same before spans are asked for.
const SETTLE: Duration = Duration::from_millis(100);

impl FileView {
    /// The `DiffRow` range the built list rows stand for. Side by side it is the list range;
    /// unified lists a changed row's sides apart, so it spans the rows' lowest and highest index.
    fn visible_rows(&self, layout: Layout) -> Option<Range<u32>> {
        let (start, end) = self.built.get();
        if layout == Layout::SideBySide {
            return Some(u32::try_from(start).ok()?..u32::try_from(end).ok()?);
        }
        let shown = pair_rows(&self.text, layout, start..end);
        let low = shown.iter().map(|row| row.row).min()?;
        let high = shown.iter().map(|row| row.row).max()?;
        Some(low..high.saturating_add(1))
    }

    /// Stores spans that arrived from the engine.
    pub(in crate::window_view::compare_screen) fn add_spans(&mut self, spans: Vec<RowSpans>) {
        self.spans.borrow_mut().insert(&self.text, spans);
    }

    /// Notes the rows built in the last frame. A change restarts a 100 ms wait, after which the
    /// window asks for the spans of unknown rows. The first frame asks for one more frame,
    /// because the list builds its rows while it paints.
    pub(in crate::window_view::compare_screen) fn watch_rows(
        &mut self,
        layout: Layout,
        window: &Window,
        cx: &mut Context<ScreenOwner>,
    ) {
        let built = self.built.get();
        if built == (0, 0) && !self.retried {
            self.retried = true;
            cx.notify();
        }
        if built == self.watched || !self.words {
            return;
        }
        self.watched = built;
        self.settle = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update(cx, |window, _| window.ask_spans(layout)).ok();
        }));
    }
}

impl ScreenOwner {
    /// Asks the engine for the spans of the visible rows it has not been asked about.
    fn ask_spans(&mut self, layout: Layout) {
        let Some(file) = self.compare.screen.as_mut().and_then(|s| s.file.as_mut()) else {
            return;
        };
        let Some(view) = file.view.as_mut().filter(|view| view.words) else {
            return;
        };
        let rows = view.visible_rows(layout);
        let asked = rows.and_then(|rows| view.spans.borrow_mut().request(rows));
        if let Some(range) = asked {
            file.control.spans(range);
        }
    }
}

impl FileView {
    /// Turns changed words on or off (the View menu does this).
    pub(in crate::window_view::compare_screen) fn set_words(&mut self, words: bool) {
        self.words = words;
        self.watched = (0, 0);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::FileView;
    use crate::compare_diff::{DiffRow, HunkView, Layout, PairFacts, PairText, ViewedKey};

    /// One hunk of `count` changed rows.
    fn one_big_hunk(count: u32) -> Arc<PairText> {
        let lines = vec![String::from("x"); count as usize];
        Arc::new(PairText {
            left_starts: Vec::new(),
            right_starts: Vec::new(),
            left: lines.clone(),
            right: lines,
            rows: (0..count).map(|at| DiffRow::Changed(at, at)).collect(),
            hunks: vec![HunkView {
                rows: 0..count,
                left_bytes: 0..0,
                right_bytes: 0..0,
                digest: None,
                added: count,
                removed: count,
            }],
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
        })
    }

    #[test]
    fn compare_screen_unified_spans_cover_the_shown_rows_deep_in_a_large_hunk() {
        let view = FileView::new(one_big_hunk(10_000), None, true);
        // Unified rows 12,000.. are the inserted sides of rows 2,000.. (deleted sides come first).
        view.built.set((12_000, 12_050));
        assert_eq!(view.visible_rows(Layout::Unified), Some(2_000..2_050));
        view.built.set((9_990, 10_010));
        assert_eq!(
            view.visible_rows(Layout::Unified),
            Some(0..10_000),
            "the turn from deleted to inserted spans the whole run"
        );
        view.built.set((100, 150));
        assert_eq!(view.visible_rows(Layout::SideBySide), Some(100..150));
    }
}
