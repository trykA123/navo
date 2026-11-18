//! Streaming syntax colours for the paged text fill; the fill worker lexes each page it reads.

use std::sync::{Arc, Mutex, PoisonError};

use sidewise_kit::languages::lines::LineLexer;
#[cfg(test)]
use sidewise_kit::languages::lines::Span;

use super::{PagedBody, PagedView};
use crate::prelude::*;
use state::{CacheKey, StateCache};

#[cfg(test)]
mod edge_tests;
mod fill;
mod page;
mod state;
pub(super) mod timing;

pub(super) use fill::{Lexing, PageInputs, fill_page};
pub(super) use page::lex_page;

/// The paged view's line lexer and its revision; a new lexer bumps the revision.
#[derive(Default)]
pub(super) struct PagedSyntax {
    pub lexer: Option<Arc<dyn LineLexer>>,
    pub rev: u64,
    cache: Arc<Mutex<StateCache>>,
}

impl PagedSyntax {
    /// Points the state cache at `key`; it clears only when the key changed (D8).
    pub(super) fn reset_cache(&self, key: CacheKey) {
        lock(&self.cache).reset(key);
    }

    /// The lexer with the shared cache and `key`, for one fill; `None` without a lexer.
    pub(super) fn lexing(&self, key: CacheKey) -> Option<Lexing> {
        let lexer = self.lexer.clone()?;
        Some(Lexing {
            lexer,
            cache: self.cache.clone(),
            key,
        })
    }
}

/// Locks the cache; a lock poisoned by a panicking fill still holds usable checkpoints.
fn lock(cache: &Mutex<StateCache>) -> std::sync::MutexGuard<'_, StateCache> {
    cache.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PagedBody {
    /// Sets the paged view's line lexer; see `PagedView::set_lexer`.
    pub(in crate::window_view::viewer) fn set_lexer(
        &self,
        lexer: Option<Arc<dyn LineLexer>>,
        cx: &mut App,
    ) {
        self.0.update(cx, |view, cx| view.set_lexer(lexer, cx));
    }
}

impl PagedView {
    /// Sets the line lexer for the text rows and refills the page when the lexer changes.
    ///
    /// The same lexer name (or `None` twice) changes nothing.
    pub(in crate::window_view::viewer) fn set_lexer(
        &mut self,
        lexer: Option<Arc<dyn LineLexer>>,
        cx: &mut Context<Self>,
    ) {
        let same = match (&self.syntax.lexer, &lexer) {
            (None, None) => true,
            (Some(old), Some(new)) => old.name() == new.name(),
            _ => false,
        };
        if same {
            return;
        }
        self.syntax.lexer = lexer;
        self.syntax.rev += 1;
        self.request_rows(cx);
    }

    /// The state cache's current key.
    #[cfg(test)]
    pub(in crate::window_view::viewer) fn cache_key(&self) -> CacheKey {
        lock(&self.syntax.cache).key()
    }

    /// The number of checkpoints in the state cache.
    #[cfg(test)]
    pub(in crate::window_view::viewer) fn cache_len(&self) -> usize {
        lock(&self.syntax.cache).len()
    }

    /// Returns the spans of the filled text row at `index`.
    #[cfg(test)]
    pub(in crate::window_view::viewer) fn row_spans(&self, index: usize) -> Vec<Span> {
        self.cache.text_spans(index)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use navo_core::paging::row_at;
    use sidewise_kit::languages::lines::{LineLanguage, LineLexer, LineState};

    use super::super::lines::{PagedKind, TextLine};
    use super::super::rows::Want;
    use super::page::PageLex;
    use super::{Lexing, PageInputs, fill_page, lex_page};
    use crate::window_view::tests::press;
    use crate::window_view::viewer::Kinds;
    use crate::window_view::viewer::tests::structured::fake_with;
    use crate::window_view::viewer::tests::{Log, hooks, press_only, window_with};
    use crate::window_view::viewer::tests::{Setup, paged, redraw, text_window};

    pub(super) fn xml() -> Arc<dyn LineLexer> {
        Arc::new(LineLanguage::Xml)
    }

    /// One row of text; `joined` marks a row whose line goes on in the next row.
    pub(super) fn row(start: u64, text: &str, joined: bool) -> TextLine {
        let end = start + text.len() as u64;
        TextLine {
            start,
            text_from: start,
            end,
            next: if joined { end } else { end + 1 },
            text: text.to_owned(),
            spans: Vec::new(),
            line: None,
        }
    }

    pub(super) fn roles(line: &TextLine) -> Vec<(std::ops::Range<usize>, &'static str)> {
        line.spans.clone()
    }

    #[test]
    fn rows_of_one_line_join_a_comment_cut_inside_its_opening() {
        let mut lines = [
            row(0, "<!-", true),
            row(3, "-x-", true),
            row(6, "->", false),
        ];
        lex_page(
            &LineLanguage::Xml,
            &mut lines,
            LineState::default(),
            u64::MAX,
        );
        assert!(lines[0].spans.is_empty(), "{:?}", lines[0].spans);
        assert_eq!(roles(&lines[1]), vec![(0..3, "comment")]);
        assert_eq!(roles(&lines[2]), vec![(0..2, "comment")]);
    }

    #[test]
    fn a_second_line_starts_from_the_state_the_first_left() {
        let mut lines = [row(0, "<!-- a", false), row(7, "b -->", false)];
        lex_page(
            &LineLanguage::Xml,
            &mut lines,
            LineState::default(),
            u64::MAX,
        );
        assert_eq!(roles(&lines[1]), vec![(0..5, "comment")]);
    }

    #[test]
    fn starts_and_end_record_the_state_at_row_offsets() {
        let mut lines = [row(0, "<!-- a", false), row(7, "b", false)];
        let lexer = LineLanguage::Xml;
        let PageLex { starts, end } = lex_page(&lexer, &mut lines, LineState::default(), u64::MAX);
        let (_, after_first) = lexer.lex("<!-- a", LineState::default());
        let carried = lexer.line_end(after_first);
        assert_eq!(starts, vec![(0, LineState::default()), (7, carried)]);
        let (_, after_second) = lexer.lex("b", carried);
        assert_eq!(end, (9, lexer.line_end(after_second)));
    }

    #[test]
    fn a_page_stops_lexing_after_its_byte_cap() {
        let big = "<a>".repeat(sidewise_kit::languages::lines::MAX_PAGE_BYTES / 3);
        let mut lines = [row(0, &big, false), row(100, "<b>", false)];
        let PageLex { starts, .. } = lex_page(
            &LineLanguage::Xml,
            &mut lines,
            LineState::default(),
            u64::MAX,
        );
        assert_eq!(starts.len(), 1);
        assert!(lines[1].spans.is_empty());
    }

    pub(super) fn page_of(
        text: &str,
        top: u64,
        lexer: Option<Arc<dyn LineLexer>>,
    ) -> Vec<TextLine> {
        let bytes = text.as_bytes();
        let want = Want {
            top,
            count: 8,
            kind: PagedKind::Text,
            index_rev: 0,
            source_rev: 0,
            lexer_rev: 0,
        };
        match fill_page(
            bytes,
            want,
            PageInputs {
                moves: &[],
                index: None,
                lexer: lexer.map(|lexer| Lexing {
                    lexer,
                    cache: Arc::default(),
                    key: (0, 0),
                }),
            },
        )
        .drawn
        {
            Ok(super::super::rows::Drawn::Text(screen)) => screen.lines,
            _ => panic!("a text page"),
        }
    }

    #[test]
    fn the_fill_colours_rows_and_a_cold_top_resyncs_from_its_head() {
        let page = page_of("<A b=\"1\">\n", 0, Some(xml()));
        assert!(page[0].spans.iter().any(|(_, role)| *role == "tag"));
        let text = "x\nend -->\n<A/>\n";
        let cold = page_of(
            text,
            row_at(text.as_bytes(), 2).expect("row").start,
            Some(xml()),
        );
        assert_eq!(cold[0].spans, vec![(0..7, "comment")]);
        assert!(page_of(text, 0, None)[0].spans.is_empty());
    }

    #[gpui_kit::test]
    fn colours_are_off_for_a_page_filled_under_an_older_lexer(cx: &mut gpui_kit::TestAppContext) {
        let (window, view, _) = text_window(cx, Setup::huge(b"<A/>\n".to_vec()));
        let view = paged(cx, &view);
        // The content sniff already picks XML for this head; start from Log so the XML switch is a change.
        view.update(cx, |view, cx| {
            view.set_lexer(Some(Arc::new(LineLanguage::Log)), cx)
        });
        redraw(cx, window);
        let base = view.read_with(cx, |view, _| view.want().lexer_rev);
        view.update(cx, |view, cx| view.set_lexer(Some(xml()), cx));
        redraw(cx, window);
        view.read_with(cx, |view, _| {
            let want = view.want();
            assert_eq!(want.lexer_rev, base + 1);
            assert!(view.cache.screen(want, 5).colours);
            let newer = Want {
                lexer_rev: base + 2,
                ..want
            };
            assert!(!view.cache.screen(newer, 5).colours);
        });
    }

    #[gpui_kit::test]
    fn a_lexer_set_after_the_first_fill_colours_the_filled_page(cx: &mut gpui_kit::TestAppContext) {
        let bytes = b"<A b=\"1\">\n<B/>\n".to_vec();
        let (window, view, _) = text_window(cx, Setup::huge(bytes));
        let view = paged(cx, &view);
        // The content sniff already picks XML for this head; start from Log so the XML switch is a change.
        view.update(cx, |view, cx| {
            view.set_lexer(Some(Arc::new(LineLanguage::Log)), cx)
        });
        redraw(cx, window);
        let before = view.read_with(cx, |view, _| view.row_spans(0));
        assert!(!before.iter().any(|(_, role)| *role == "tag"), "{before:?}");
        view.update(cx, |view, cx| view.set_lexer(Some(xml()), cx));
        redraw(cx, window);
        let spans = view.read_with(cx, |view, _| view.row_spans(0));
        assert!(spans.iter().any(|(_, role)| *role == "tag"), "{spans:?}");
        let fills = view.read_with(cx, |view, _| view.fills());
        view.update(cx, |view, cx| view.set_lexer(Some(xml()), cx));
        redraw(cx, window);
        assert_eq!(view.read_with(cx, |view, _| view.fills()), fills);
    }

    #[gpui_kit::test]
    fn a_big_arxml_opened_as_text_comes_up_coloured(cx: &mut gpui_kit::TestAppContext) {
        let bytes = b"<A b=\"1\"><!-- c --></A>\n".to_vec();
        let files = hooks(Setup::huge(bytes), &Log::default());
        let len = crate::window_view::viewer::EDIT_LIMIT + 1;
        let (window, view) = window_with(cx, fake_with("big.arxml", len), files, Kinds::builtin());
        press(cx, window, &view, "down");
        press_only(cx, window, "f3");
        redraw(cx, window);
        press_only(cx, window, "alt-1");
        redraw(cx, window);
        let view = paged(cx, &view);
        let spans = view.read_with(cx, |view, _| view.row_spans(0));
        for role in ["tag", "attribute"] {
            assert!(spans.iter().any(|(_, r)| *r == role), "{role}: {spans:?}");
        }
    }
}
