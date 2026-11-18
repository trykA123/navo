//! The paged view's lexer-state cache (D8): checkpoints that let a page start mid-comment.

use std::collections::{HashMap, VecDeque};

use sidewise_kit::languages::lines::LineState;

/// Most checkpoints the cache holds; the oldest half goes when it is full (D8).
const MAX_ENTRIES: usize = 16_384;

/// `(source_rev, lexer_rev)`: the cache is valid for one source and one lexer only.
pub(super) type CacheKey = (u64, u64);

/// Lexer states by row start offset, valid under one `CacheKey`.
#[derive(Default)]
pub(super) struct StateCache {
    key: CacheKey,
    states: HashMap<u64, LineState>,
    order: VecDeque<u64>,
}

impl StateCache {
    /// Switches to `key`, clearing every checkpoint only when the key differs.
    pub(super) fn reset(&mut self, key: CacheKey) {
        if self.key != key {
            self.key = key;
            self.states.clear();
            self.order.clear();
        }
    }

    /// The state recorded at `offset`; `None` when absent or under another key.
    pub(super) fn get(&self, key: CacheKey, offset: u64) -> Option<LineState> {
        if self.key != key {
            return None;
        }
        self.states.get(&offset).copied()
    }

    /// Records `state` at `offset`; dropped when `key` is not the current key.
    ///
    /// A full cache first drops its oldest half (by first insertion).
    pub(super) fn put(&mut self, key: CacheKey, offset: u64, state: LineState) {
        if self.key != key {
            return;
        }
        if let Some(old) = self.states.get_mut(&offset) {
            *old = state;
            return;
        }
        if self.states.len() >= MAX_ENTRIES {
            for oldest in self.order.drain(..MAX_ENTRIES / 2) {
                self.states.remove(&oldest);
            }
        }
        self.states.insert(offset, state);
        self.order.push_back(offset);
    }

    /// The current key.
    #[cfg(test)]
    pub(super) fn key(&self) -> CacheKey {
        self.key
    }

    /// The number of checkpoints held.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.states.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use navo_core::paging::row_at;
    use sidewise_kit::languages::lines::{LineLanguage, LineLexer, LineState};

    use super::super::super::lines::PagedKind;
    use super::super::super::moves::Move;
    use super::super::super::rows::{Drawn, FillResult, Want};
    use super::super::{Lexing, PageInputs, fill_page};
    use super::{MAX_ENTRIES, StateCache};
    use crate::window_view::viewer::tests::{Setup, paged, redraw, text_window};

    const KEY: (u64, u64) = (0, 1);

    fn xml() -> Arc<dyn LineLexer> {
        Arc::new(LineLanguage::Xml)
    }

    fn shared() -> Arc<Mutex<StateCache>> {
        let cache = Arc::new(Mutex::new(StateCache::default()));
        cache.lock().expect("lock").reset(KEY);
        cache
    }

    fn lexing(cache: &Arc<Mutex<StateCache>>, key: (u64, u64)) -> Option<Lexing> {
        Some(Lexing {
            lexer: xml(),
            cache: cache.clone(),
            key,
        })
    }

    /// Fills a page of `text` at `top` (8 rows) after `moves`, caching under `KEY`.
    fn run(text: &str, top: u64, moves: &[Move], cache: &Arc<Mutex<StateCache>>) -> FillResult {
        run_under(text, top, moves, (cache, KEY))
    }

    /// Like `run`, with the cache and the key the fill stores under given.
    fn run_under(
        text: &str,
        top: u64,
        moves: &[Move],
        store: (&Arc<Mutex<StateCache>>, (u64, u64)),
    ) -> FillResult {
        let (cache, key) = store;
        let want = Want {
            top,
            count: 8,
            kind: PagedKind::Text,
            index_rev: 0,
            source_rev: key.0,
            lexer_rev: key.1,
        };
        let with = PageInputs {
            moves,
            index: None,
            lexer: lexing(cache, key),
        };
        fill_page(text.as_bytes(), want, with)
    }

    fn spans_of(filled: &FillResult, row: usize) -> Vec<(std::ops::Range<usize>, &'static str)> {
        match &filled.drawn {
            Ok(Drawn::Text(screen)) => screen.lines[row].spans.clone(),
            _ => panic!("a text page"),
        }
    }

    fn len(cache: &Arc<Mutex<StateCache>>) -> usize {
        cache.lock().expect("lock").len()
    }

    fn rows_of(first: &str, filler: &str, times: usize) -> String {
        format!("{first}{}", format!("{filler}\n").repeat(times))
    }

    #[test]
    fn a_key_change_clears_and_a_put_under_another_key_is_dropped() {
        let mut cache = StateCache::default();
        cache.reset(KEY);
        cache.put(KEY, 5, LineState(7));
        assert_eq!(cache.get(KEY, 5), Some(LineState(7)));
        cache.reset(KEY);
        assert_eq!(cache.get(KEY, 5), Some(LineState(7)));
        cache.put((0, 2), 6, LineState(1));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get((0, 2), 5), None);
        cache.reset((0, 2));
        assert_eq!((cache.key(), cache.len()), ((0, 2), 0));
    }

    #[test]
    fn twenty_thousand_puts_keep_at_most_the_cap() {
        let mut cache = StateCache::default();
        cache.reset(KEY);
        for offset in 0..20_000 {
            cache.put(KEY, offset, LineState(1));
        }
        assert!(cache.len() <= MAX_ENTRIES, "{}", cache.len());
        assert!(cache.len() > MAX_ENTRIES / 2);
        assert_eq!(cache.get(KEY, 19_999), Some(LineState(1)));
        assert_eq!(cache.get(KEY, 0), None);
    }

    #[test]
    fn the_next_page_starts_in_the_comment_the_last_row_opened() {
        let text = format!(
            "{}<!-- open\nplain text\n<b/>\n{}",
            "<a/>\n".repeat(7),
            "x\n".repeat(40)
        );
        let cache = shared();
        run(&text, 0, &[], &cache);
        let next = run(&text, 0, &[Move::Rows(8)], &cache);
        assert_eq!(spans_of(&next, 0), vec![(0..10, "comment")]);
    }

    #[test]
    fn queued_moves_resync_a_cold_top_and_the_lookup_uses_the_resolved_top() {
        let text = format!(
            "{}words\nwords -->\n<x/>\n{}",
            "words\n".repeat(24),
            "x\n".repeat(40)
        );
        let cache = shared();
        run(&text, 0, &[], &cache);
        let before = len(&cache);
        let landed = run(&text, 0, &[Move::Rows(8); 3], &cache);
        assert_eq!(landed.top, 24 * 6);
        assert_eq!(spans_of(&landed, 0), vec![(0..5, "comment")]);
        assert_eq!(spans_of(&landed, 1), vec![(0..9, "comment")]);
        assert!(len(&cache) > before);
    }

    #[test]
    fn a_stale_completion_draws_its_rows_but_stores_nothing() {
        let cache = shared();
        cache.lock().expect("lock").reset((0, 2));
        let filled = run_under("<a/>\n<b/>\n", 0, &[], (&cache, (0, 1)));
        assert!(!spans_of(&filled, 0).is_empty());
        assert_eq!(len(&cache), 0);
    }

    #[test]
    fn top_zero_starts_from_the_default_state_not_from_resync() {
        let text = "plain words\n--> x\n<a>\n";
        let filled = run(text, 0, &[], &shared());
        assert!(spans_of(&filled, 2).iter().any(|(_, role)| *role == "tag"));
    }

    #[test]
    fn a_cold_page_with_no_resync_guess_draws_plain_and_stores_nothing() {
        let text = rows_of("", "plain words", 40);
        let cache = shared();
        let top = row_at(text.as_bytes(), 10).expect("row").start;
        let filled = run(&text, top, &[], &cache);
        assert!(spans_of(&filled, 0).is_empty());
        assert_eq!(len(&cache), 0);
    }

    #[test]
    fn a_capped_page_stores_checkpoints_only_for_the_rows_it_lexed() {
        use super::super::super::lines::TextLine;
        use super::super::lex_page;
        let big = "<a>".repeat(sidewise_kit::languages::lines::MAX_PAGE_BYTES / 3);
        let line = |start: u64, text: &str| TextLine {
            start,
            text_from: start,
            end: start + text.len() as u64,
            next: start + text.len() as u64 + 1,
            text: text.to_owned(),
            spans: Vec::new(),
            line: None,
        };
        let first = line(0, &big);
        let after = first.next;
        let mut lines = [first, line(after, "<b>")];
        let page = lex_page(
            &LineLanguage::Xml,
            &mut lines,
            LineState::default(),
            u64::MAX,
        );
        let offsets: Vec<u64> = page.starts.iter().map(|(at, _)| *at).collect();
        assert_eq!(offsets, vec![0]);
        assert_eq!(page.end.0, after);
        assert!(lines[1].spans.is_empty());
    }

    #[gpui_kit::test]
    fn set_lexer_and_a_source_change_reset_the_cache_before_any_fill(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let bytes = b"<A b=\"1\">\n<B/>\n".to_vec();
        let (window, view, _) = text_window(cx, Setup::huge(bytes));
        let view = paged(cx, &view);
        // The content sniff already picks XML for this head; start from Log so the XML switch is a change.
        view.update(cx, |view, cx| {
            view.set_lexer(Some(Arc::new(LineLanguage::Log)), cx)
        });
        redraw(cx, window);
        let rev = view.read_with(cx, |view, _| view.source_rev);
        let fills = view.read_with(cx, |view, _| view.fills());
        let base = view.read_with(cx, |view, _| view.want().lexer_rev);
        view.update(cx, |view, cx| {
            view.set_lexer(Some(xml()), cx);
            view.set_lexer(Some(Arc::new(LineLanguage::Json)), cx);
        });
        assert_eq!(
            view.read_with(cx, |view, _| view.cache_key()),
            (rev, base + 2)
        );
        redraw(cx, window);
        assert_eq!(
            view.read_with(cx, |view, _| view.cache_key()),
            (rev, base + 2)
        );
        assert_eq!(view.read_with(cx, |view, _| view.fills()), fills + 2);
        view.update(cx, |view, cx| view.set_lexer(Some(xml()), cx));
        view.update(cx, |view, cx| {
            view.source_rev += 1;
            view.request_rows(cx);
        });
        assert_eq!(
            view.read_with(cx, |view, _| view.cache_key()),
            (rev + 1, base + 3)
        );
        redraw(cx, window);
        assert!(view.read_with(cx, |view, _| view.cache_len()) > 0);
    }
}
