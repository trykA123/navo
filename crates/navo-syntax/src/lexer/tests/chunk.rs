//! The chunk API (`Lexer::chunk`): the lexer reports the exact state where a chunk stops.

use super::*;
use crate::{ChunkState, Inside};

pub(super) fn lexer(lang: &Language) -> Lexer {
    Lexer::new(lang)
}

fn code(mid_word: bool) -> ChunkState {
    ChunkState {
        inside: Inside::Code,
        mid_word,
        partial: 0,
    }
}

fn inside(index: u16, escaped: bool) -> ChunkState {
    ChunkState {
        inside: Inside::Delimited { index, escaped },
        mid_word: false,
        partial: 0,
    }
}

/// Lexes `chunks` as one line and returns every token shifted to the line; a comment or string
/// that runs over a cut comes back as two tokens, so touching tokens of one kind are joined.
pub(super) fn chunked(lang: &Language, chunks: &[&str]) -> (Vec<(usize, usize, K)>, ChunkState) {
    let lexer = lexer(lang);
    let (mut out, mut state, mut start) =
        (Vec::<(usize, usize, K)>::new(), ChunkState::default(), 0);
    for (i, chunk) in chunks.iter().enumerate() {
        let (tokens, next) = lexer.chunk(chunk, state, i + 1 == chunks.len());
        for t in &tokens {
            let (from, to) = (start + t.range.start, start + t.range.end);
            match out.last_mut() {
                Some(last) if last.1 == from && last.2 == t.kind => last.1 = to,
                _ => out.push((from, to, t.kind)),
            }
        }
        state = next;
        start += chunk.len();
    }
    (out, state)
}

pub(super) fn whole(lang: &Language, line: &str) -> Vec<(usize, usize, K)> {
    let (tokens, _) = lexer(lang).line(line, LineState::Normal);
    tokens
        .iter()
        .map(|t| (t.range.start, t.range.end, t.kind))
        .collect()
}

#[test]
fn a_chunk_that_may_continue_reports_the_comment_it_stops_in() {
    let lexer = lexer(&sample());
    assert_eq!(
        lexer.chunk("if // x", code(false), false).1.inside,
        Inside::LineComment
    );
    assert_eq!(lexer.chunk("if // x", code(false), true).1, code(false));
    let (tokens, next) = lexer.chunk(
        " more",
        ChunkState {
            inside: Inside::LineComment,
            mid_word: false,
            partial: 0,
        },
        false,
    );
    assert_eq!(tokens.len(), 1);
    assert_eq!(next.inside, Inside::LineComment);
    assert_eq!(lexer.chunk(" end", next, true).1, code(false));
}

#[test]
fn a_string_with_a_pending_escape_keeps_the_escaped_char() {
    let mut lang = sample();
    lang.delimiters[0].multiline = true;
    let (state, line) = (inside(0, true), "\"abc\\\" if\"");
    let lexer = lexer(&lang);
    let (_, next) = lexer.chunk("\"abc\\", ChunkState::default(), false);
    assert_eq!(next, state);
    assert_eq!(
        chunked(&lang, &["\"abc\\", "\" if\""]).0,
        whole(&lang, line)
    );
    assert_eq!(lexer.chunk("", next, false).1, state);
    assert_eq!(lexer.chunk("", next, true).1, inside(0, false));
    assert_eq!(lexer.chunk("", ChunkState::default(), true).1, code(false));
}

#[test]
fn a_one_line_string_continues_into_the_next_chunk_but_not_past_the_line() {
    let lang = sample();
    let (_, next) = lexer(&lang).chunk("if \"ab", ChunkState::default(), false);
    assert_eq!(next, inside(0, false));
    assert_eq!(
        chunked(&lang, &["if \"ab", "c\" if"]).0,
        whole(&lang, "if \"abc\" if")
    );
    let (_, end) = lexer(&lang).chunk("\"ab", ChunkState::default(), true);
    assert_eq!(end, code(false), "a one-line string ends with its line");
}

#[test]
fn case_distinct_delimiters_report_the_index_the_lexer_matched() {
    let mut lang = sample();
    lang.case_ignored = true;
    lang.delimiters = vec![
        delimiter("S", "!", None, false),
        delimiter("s", "?", None, false),
    ];
    let (_, next) = lexer(&lang).chunk("sabc", ChunkState::default(), false);
    assert_eq!(next, inside(1, false));
    let split = chunked(&lang, &["sabc", "? if"]).0;
    assert_eq!(split, [(0, 5, K::String), (6, 8, K::Keyword)]);
}

#[test]
fn a_chunk_that_ends_in_a_word_makes_the_next_chunk_start_inside_it() {
    let mut lang = sample();
    lang.line_comments = owned(&["dnl"]);
    let lexer = lexer(&lang);
    assert_eq!(lexer.chunk("x", ChunkState::default(), false).1, code(true));
    assert_eq!(
        lexer.chunk("x ", ChunkState::default(), false).1,
        code(false)
    );
    assert_eq!(lexer.chunk("x", ChunkState::default(), true).1, code(false));
    assert_eq!(lexer.chunk("", code(true), false).1, code(true));
    assert_eq!(lexer.chunk("é", ChunkState::default(), false).1, code(true));
    let (tokens, next) = lexer.chunk("dnl tail", code(true), true);
    assert!(tokens.is_empty(), "{tokens:?}");
    assert_eq!(next, code(false));
    assert_eq!(
        chunked(&lang, &["x", "dnl tail"]).0,
        whole(&lang, "xdnl tail")
    );
    assert_eq!(
        chunked(&lang, &["x ", "dnl tail"]).0,
        whole(&lang, "x dnl tail")
    );
}

#[test]
fn a_mid_word_chunk_start_blocks_a_keyword_and_a_boundary_only_token() {
    let lang = sample();
    let lexer = lexer(&lang);
    assert!(lexer.chunk("if", code(true), true).0.is_empty());
    assert!(lexer.chunk("42", code(true), true).0.is_empty());
    assert!(lexer.chunk("dsGetX", code(true), true).0.is_empty());
    let (tokens, _) = lexer.chunk("== if", code(true), true);
    assert_eq!(tokens.len(), 2, "an operator and a keyword after the word");
}

#[test]
fn the_state_is_tracked_past_the_token_cap() {
    let lang = sample();
    let line = format!("{}\"open", "if ".repeat(20_000));
    let (tokens, next) = lexer(&lang).chunk(&line, ChunkState::default(), false);
    assert_eq!(tokens.len(), MAX_TOKENS);
    assert_eq!(next, inside(0, false));
}

#[test]
fn the_last_chunk_gives_the_line_result() {
    let lang = sample();
    let lexer = lexer(&lang);
    for line in [
        "if /* x",
        "'''ab",
        "\"ab\\",
        "x == if // c",
        "(* if *) TRUE",
    ] {
        let (tokens, state) = lexer.line(line, LineState::Normal);
        let (chunk, next) = lexer.chunk(line, ChunkState::default(), true);
        assert_eq!((tokens, state), (chunk, next.line_state()), "{line}");
    }
}

#[test]
fn a_short_keyword_before_non_word_unicode_is_a_keyword() {
    assert_eq!(tokens(&sample(), "if\u{201e}"), [("if", K::Keyword)]);
}

#[test]
fn a_cut_inside_a_closer_still_closes_the_block_or_string() {
    let lang = sample();
    for (chunks, line) in [
        (&["if /* a *", "/ if"][..], "if /* a */ if"),
        (&["if (* a *", ") if"], "if (* a *) if"),
        (&["'''ab''", "' if"], "'''ab''' if"),
        (&["'''ab'", "'' if"], "'''ab''' if"),
        (&["/* a *", "*", "/ if"], "/* a **/ if"),
        (&["/* a *", "/", " if"], "/* a */ if"),
    ] {
        assert_eq!(chunked(&lang, chunks).0, whole(&lang, line), "{chunks:?}");
        assert_eq!(chunked(&lang, chunks).1.inside, Inside::Code, "{chunks:?}");
    }
}

/// The tokens from byte `at` on: a chunk cannot recolour bytes an earlier chunk returned.
fn from(tokens: Vec<(usize, usize, K)>, at: usize) -> Vec<(usize, usize, K)> {
    tokens
        .into_iter()
        .filter(|t| t.1 > at)
        .map(|t| (t.0.max(at), t.1, t.2))
        .collect()
}

#[test]
fn a_cut_inside_an_opener_still_opens_the_comment_or_string() {
    let lang = sample();
    for (chunks, line) in [
        (&["if /", "/ x if"][..], "if // x if"),
        (&["if /", "* x */ if"], "if /* x */ if"),
        (&["if (", "* x *) if"], "if (* x *) if"),
        (&["if ''", "'ab''' if"], "if '''ab''' if"),
        (&["if '", "'", "'ab''' if"], "if '''ab''' if"),
        (&["if #", " x if"], "if # x if"),
    ] {
        let cut = line.len() - chunks.last().map_or(0, |last| last.len());
        let split = from(chunked(&lang, chunks).0, cut);
        assert_eq!(split, from(whole(&lang, line), cut), "{chunks:?}");
    }
}

#[test]
fn a_block_left_open_by_a_cut_opener_is_a_block_state() {
    let lang = sample();
    let (_, next) = chunked(&lang, &["if /", "* open"]);
    assert_eq!(next.inside, Inside::Block(0));
}
