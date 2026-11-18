//! Log lexer span and streaming contract checks.
use super::*;

/// Lexes one complete line (a chunk that ends its line) and returns its spans.
fn lex_spans(line: &str) -> Vec<Span> {
    LineLanguage::Log.lex_last(line, LineState(0)).0
}

/// Lexes one chunk that may continue and returns its spans.
fn plain_spans(chunk: &str) -> Vec<Span> {
    LineLanguage::Log.lex(chunk, LineState(0)).0
}

#[test]
fn colours_upper_case_level_in_brackets() {
    assert_eq!(lex_spans("[ERROR] x"), vec![(1..6, "title")]);
}

#[test]
fn colours_lower_case_level_before_a_colon() {
    assert_eq!(lex_spans("error: x"), vec![(0..5, "title")]);
}

#[test]
fn lower_case_and_capitalised_levels_need_colon_bracket_or_pipe() {
    assert_eq!(lex_spans("Error: x"), vec![(0..5, "title")]);
    assert_eq!(lex_spans("warning] x"), vec![(0..7, "keyword")]);
    assert_eq!(lex_spans("info| x"), vec![(0..4, "function")]);
    assert!(lex_spans("error x").is_empty());
    assert!(lex_spans("Error, x").is_empty());
}

#[test]
fn skips_words_that_match_no_level_whole() {
    assert!(lex_spans("errors found").is_empty());
    assert!(lex_spans("v1.2.3").is_empty());
    assert!(lex_spans("file.txt").is_empty());
    assert!(lex_spans("ERROR.LOG").is_empty());
}

#[test]
fn single_quotes_need_a_boundary_and_a_close() {
    assert_eq!(
        lex_spans("don't stop 'now'"),
        vec![(11..16, "string.special")]
    );
}

#[test]
fn carriage_return_is_plain() {
    assert_eq!(lex_spans("[ERROR] x\r"), vec![(1..6, "title")]);
    assert_eq!(lex_spans("[ERROR]\r"), vec![(1..6, "title")]);
    assert_eq!(lex_spans("42\r"), vec![(0..2, "number")]);
}

#[test]
fn maps_every_level_word_to_its_d5_role() {
    for (word, role) in [
        ("ERROR", "title"),
        ("ERR", "title"),
        ("FATAL", "title"),
        ("CRITICAL", "title"),
        ("CRIT", "title"),
        ("FAIL", "title"),
        ("FAILED", "title"),
        ("FAILURE", "title"),
        ("PANIC", "title"),
        ("EXCEPTION", "title"),
        ("WARN", "keyword"),
        ("WARNING", "keyword"),
        ("INFO", "function"),
        ("NOTICE", "function"),
        ("DEBUG", "comment.doc"),
        ("TRACE", "comment.doc"),
        ("VERBOSE", "comment.doc"),
        ("DBG", "comment.doc"),
    ] {
        let line = format!("{word} x");
        assert_eq!(lex_spans(&line), vec![(0..word.len(), role)], "{word}");
    }
}

#[test]
fn carries_double_strings_and_escapes_across_chunks() {
    let lexer = LineLanguage::Log;
    assert_eq!(lex_spans("\"a\\\"b\" x"), vec![(0..6, "string")]);
    let (spans, state) = lexer.lex("\"a\\", LineState(0));
    assert_eq!(spans, vec![(0..3, "string")]);
    assert_eq!(state, LineState(1 | (1 << 16)));
    assert_eq!(
        lexer.lex("\"b\" ", state),
        (vec![(0..3, "string")], LineState(0))
    );
    assert_eq!(
        lexer.lex("\"open", LineState(0)),
        (vec![(0..5, "string")], LineState(1))
    );
    let (_, open) = lexer.lex("\"open", LineState(0));
    assert_eq!(lexer.line_end(open), LineState(0));
    assert_eq!(
        lexer.lex("\"x\" ", lexer.line_end(open)).0,
        vec![(0..3, "string")]
    );
    assert!(lex_spans("abc\"def").is_empty());
}

#[test]
fn single_quotes_close_only_on_a_boundary_within_the_limit() {
    assert_eq!(lex_spans("'now' x"), vec![(0..5, "string.special")]);
    assert!(lex_spans("'now'x").is_empty());
    let longest = format!("'{}'", "a".repeat(198));
    assert_eq!(lex_spans(&longest), vec![(0..200, "string.special")]);
    let too_long = format!("'{}'", "a".repeat(199));
    assert!(lex_spans(&too_long).is_empty());
    assert!(lex_spans("abc'def'").is_empty());
}

#[test]
fn a_chunk_starting_inside_a_word_starts_no_token() {
    let lexer = LineLanguage::Log;
    let (spans, state) = lexer.lex("ERR", LineState(0));
    assert!(spans.is_empty());
    assert_eq!(state, LineState(2 | (1 << 17)));
    assert!(lexer.lex("OR: x", state).0.is_empty());
    let (spans, state) = lexer.lex("Error", LineState(0));
    assert!(spans.is_empty());
    assert!(lexer.lex(": x", state).0.is_empty());
}

#[test]
fn colours_hex_and_decimal_numbers() {
    assert_eq!(lex_spans("code 0x1F at 12ms"), vec![(5..9, "number")]);
    assert_eq!(lex_spans("1.2.3"), vec![(0..5, "number")]);
    assert_eq!(lex_spans("id 7FFE12AB ok"), vec![(3..11, "number")]);
    assert!(lex_spans("id DEADBEEF ok").is_empty());
    assert_eq!(lex_spans("café 42"), vec![(6..8, "number")]);
}

#[test]
fn a_minus_starts_a_number_only_at_a_boundary() {
    assert_eq!(lex_spans("x -5"), vec![(2..4, "number")]);
    assert_eq!(lex_spans("a-5"), vec![(2..3, "number")]);
}

#[test]
fn hex_needs_digits_and_a_boundary() {
    assert_eq!(lex_spans("0X1f"), vec![(0..4, "number")]);
    assert!(lex_spans("0x").is_empty());
    assert!(lex_spans("0x1Fg").is_empty());
    assert_eq!(lex_spans("0123456789ABCDEF ok"), vec![(0..16, "number")]);
    assert_eq!(lex_spans("12345678"), vec![(0..8, "number")]);
    assert!(lex_spans("1234ABCD5").is_empty());
}

#[test]
fn decimal_forms_include_dots_and_exponents() {
    assert_eq!(lex_spans("1.5e-3 x"), vec![(0..6, "number")]);
    assert_eq!(lex_spans("-1.2e+3"), vec![(0..7, "number")]);
    assert_eq!(lex_spans("1e5"), vec![(0..3, "number")]);
    assert!(lex_spans("1e").is_empty());
    assert!(lex_spans("12ms 3rd").is_empty());
}

#[test]
fn stops_emitting_at_the_span_cap_but_keeps_scanning() {
    let line = "ERROR ".repeat(MAX_SPANS + 1);
    let (spans, state) = LineLanguage::Log.lex(&line, LineState(0));
    assert_eq!(spans.len(), MAX_SPANS);
    let last = (MAX_SPANS - 1) * 6;
    assert_eq!(spans[MAX_SPANS - 1], (last..last + 5, "title"));
    assert_eq!(state, LineState(0));
    let line = "42 ".repeat(MAX_SPANS + 1);
    let (spans, _) = LineLanguage::Log.lex(&line, LineState(0));
    assert_eq!(spans.len(), MAX_SPANS);
    assert_eq!(spans[0], (0..2, "number"));
}

#[test]
fn a_level_word_at_the_line_end_needs_lex_last() {
    assert_eq!(lex_spans("Status: ERROR"), vec![(8..13, "title")]);
    assert!(plain_spans("Status: ERROR").is_empty());
    assert_eq!(plain_spans("Status: ERROR x"), vec![(8..13, "title")]);
    assert!(lex_spans("Status: error").is_empty());
}

#[test]
fn a_closing_quote_at_the_line_end_needs_lex_last() {
    assert_eq!(lex_spans("x 'now'"), vec![(2..7, "string.special")]);
    assert!(plain_spans("x 'now'").is_empty());
}

#[test]
fn log_lines_are_line_local() {
    assert!(LineLanguage::Log.line_local());
    assert!(!LineLanguage::Xml.line_local());
    assert!(!LineLanguage::Json.line_local());
}

#[test]
fn the_new_roles_are_listed() {
    assert!(ROLES.contains(&"comment.doc"));
    assert!(ROLES.contains(&"string.special"));
}

#[test]
fn a_date_or_number_at_the_chunk_end_gives_no_plain_span() {
    assert!(plain_spans("2026-11-18").is_empty());
    assert!(plain_spans("x 42").is_empty());
    assert!(plain_spans("x http://a.b/c").is_empty());
    assert_eq!(lex_spans("2026-11-18"), vec![(0..10, "attribute")]);
    assert_eq!(plain_spans("2026-11-18 x"), vec![(0..10, "attribute")]);
}
