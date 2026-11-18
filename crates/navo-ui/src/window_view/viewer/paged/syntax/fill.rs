//! Fills one paged text page and colours its rows with the view's line lexer (D8, D7).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use navo_core::paging::{ByteSource, LineIndex};
use sidewise_kit::languages::lines::{LineLexer, LineState};

use super::super::lines::{TextLine, starts_line};
use super::super::moves::Move;
use super::super::rows::{Drawn, FillResult, Want, fill};
use super::state::{CacheKey, StateCache};
use super::{lex_page, lock};

/// Longest page head handed to `LineLexer::resync`, in bytes (D8).
const RESYNC_HEAD: usize = 64 * 1024;

/// A line lexer with the state cache and the key (the sent `Want`'s) a fill stores under.
pub(in crate::window_view::viewer::paged) struct Lexing {
    pub(super) lexer: Arc<dyn LineLexer>,
    pub(super) cache: Arc<Mutex<StateCache>>,
    pub(super) key: CacheKey,
}

/// The first `RESYNC_HEAD` bytes of the row text: a newline ends each logical line, cut rows join.
fn page_head(lines: &[TextLine]) -> String {
    let mut head = String::new();
    for line in lines {
        if head.len() >= RESYNC_HEAD {
            break;
        }
        head.push_str(&line.text);
        if line.next > line.end {
            head.push('\n');
        }
    }
    let mut cut = head.len().min(RESYNC_HEAD);
    while !head.is_char_boundary(cut) {
        cut -= 1;
    }
    head.truncate(cut);
    head
}

/// What a page fill needs besides the source and the wanted rows.
pub(in crate::window_view::viewer::paged) struct PageInputs<'a> {
    pub moves: &'a [Move],
    pub index: Option<&'a LineIndex>,
    pub lexer: Option<Lexing>,
}

/// Reads a page like `fill` and, for a text page with a lexer, colours its rows.
///
/// A page at offset 0 starts from the default state; a top the cache knows starts from its
/// state. Any other top that starts a line starts from `resync` of its head. A cold top inside
/// a line (a continuation row) leaves that line's rows plain, up to the line end, and the next
/// line starts as above (D7, no lookback). A `None` start leaves the rows plain and stores
/// nothing. After lexing, every row start and the page end are stored (D8).
pub(in crate::window_view::viewer::paged) fn fill_page<S: ByteSource + ?Sized>(
    source: &S,
    want: Want,
    with: PageInputs<'_>,
) -> FillResult {
    let began = Instant::now();
    let mut filled = fill(source, want, with.moves, with.index);
    filled.fill_us = micros(began);
    let lexed = Instant::now();
    if let (Some(with), Ok(Drawn::Text(screen))) = (with.lexer, &mut filled.drawn) {
        let Lexing { lexer, cache, key } = &with;
        if let Some((from, start)) = start_at(source, &with, filled.top, &screen.lines) {
            let page = lex_page(&**lexer, &mut screen.lines[from..], start, source.len());
            let mut cache = lock(cache);
            for (offset, state) in page.starts.into_iter().chain([page.end]) {
                cache.put(*key, offset, state);
            }
        }
        filled.lex_us = micros(lexed);
    }
    filled
}

/// Finds the first row to lex and its start state, or `None` when the whole page stays plain.
///
/// Rows before the returned index are the rest of a line the page joined in the middle.
fn start_at<S: ByteSource + ?Sized>(
    source: &S,
    with: &Lexing,
    top: u64,
    lines: &[TextLine],
) -> Option<(usize, LineState)> {
    let Lexing { lexer, cache, key } = with;
    if top == 0 {
        return Some((0, LineState::default()));
    }
    if let Some(state) = lock(cache).get(*key, top) {
        return Some((0, state));
    }
    // A failed read counts as a continuation: plain is the safe side.
    let from = if starts_line(source, top).unwrap_or(false) {
        0
    } else {
        lines.iter().position(|line| line.next > line.end)? + 1
    };
    let first = lines.get(from)?;
    if from > 0
        && let Some(state) = lock(cache).get(*key, first.start)
    {
        return Some((from, state));
    }
    lexer
        .resync(&page_head(&lines[from..]))
        .map(|state| (from, state))
}

/// Microseconds since `from`, saturating at `u64::MAX`.
fn micros(from: Instant) -> u64 {
    u64::try_from(from.elapsed().as_micros()).unwrap_or(u64::MAX)
}
