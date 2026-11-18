//! JSON token and streaming string contract checks.
use super::*;
use proptest::prelude::*;

#[test]
fn colours_json_tokens() {
    let (spans, _) = LineLanguage::Json.lex("{\"a\": [1, -2.5e3, true, null]}", LineState(0));
    assert_eq!(
        spans,
        vec![
            (0..1, "punctuation.bracket"),
            (1..4, "property"),
            (4..5, "punctuation.delimiter"),
            (6..7, "punctuation.bracket"),
            (7..8, "number"),
            (8..9, "punctuation.delimiter"),
            (10..16, "number"),
            (16..17, "punctuation.delimiter"),
            (18..22, "boolean"),
            (22..23, "punctuation.delimiter"),
            (24..28, "constant"),
            (28..29, "punctuation.bracket"),
            (29..30, "punctuation.bracket"),
        ]
    );
}

#[test]
fn carries_strings_and_clears_escape_at_line_end() {
    let lexer = LineLanguage::Json;
    let (first, state) = lexer.lex("\"a\\", LineState(0));
    assert_eq!(first, vec![(0..3, "string")]);
    assert_eq!(
        lexer.lex("\"b\"", state),
        (vec![(0..3, "string")], LineState(0))
    );
    let (spans, state) = lexer.lex("\"b\"", lexer.line_end(state));
    assert_eq!(spans, vec![(0..1, "string"), (2..3, "string")]);
    assert_eq!(state, LineState(1));
    let (_, state) = lexer.lex("\"first", LineState(0));
    assert_eq!(
        lexer.lex("second\"", lexer.line_end(state)),
        (vec![(0..7, "string")], LineState(0))
    );
}

proptest! {
    #[test]
    fn streaming_spans_obey_contract(lines in prop::collection::vec("[a-zA-Z0-9 {}\\[\\]:,\".\\\\+_/*-]{0,80}", 5), cut_seed in any::<usize>()) {
        let lexer = LineLanguage::Json;
        let mut state = LineState(0);
        for line in lines {
            let (spans, next) = lexer.lex(&line, state);
            prop_assert!(check_spans(&line, &spans).is_ok());
            let cut = cut_seed % (line.len() + 1);
            prop_assert!(check_split(&lexer, &line, &[cut], state, &[("string", "property")]).is_ok(), "line {:?}, cut {}, state {:?}: {:?}", line, cut, state, check_split(&lexer, &line, &[cut], state, &[("string", "property")]));
            state = lexer.line_end(next);
        }
    }
}

#[test]
fn colours_keys_and_accepts_split_key() {
    let lexer = LineLanguage::Json;
    assert_eq!(
        lexer.lex("\"k\" : \"v\"", LineState(0)).0,
        vec![
            (0..3, "property"),
            (4..5, "punctuation.delimiter"),
            (6..9, "string")
        ]
    );
    assert!(
        check_split(
            &lexer,
            "\"k\" : 1",
            &[3],
            LineState(0),
            &[("string", "property")]
        )
        .is_ok()
    );
}

#[test]
fn carries_comments_across_chunks_and_lines() {
    let lexer = LineLanguage::Json;
    let (spans, state) = lexer.lex("/", LineState(0));
    assert!(spans.is_empty());
    assert_eq!(state.prefix_len(), 1);
    let (spans, state) = lexer.lex("/ c", state);
    assert_eq!(spans, vec![(0..3, "comment")]);
    assert_eq!(lexer.lex(" more", state).0, vec![(0..5, "comment")]);
    assert_eq!(lexer.line_end(state), LineState(0));
    let (spans, state) = lexer.lex("/* c", LineState(0));
    assert_eq!(spans, vec![(0..4, "comment")]);
    let (spans, state) = lexer.lex("*", lexer.line_end(state));
    assert_eq!(spans, vec![(0..1, "comment")]);
    assert_eq!(
        lexer.lex("/ 1", state),
        (
            vec![(0..1, "comment"), (2..3, "number")],
            LineState(1 << 20)
        )
    );
    assert!(
        check_split(
            &lexer,
            "// c",
            &[1, 3],
            LineState(0),
            &[("string", "property")]
        )
        .is_ok()
    );
    assert!(
        check_split(
            &lexer,
            "/* c */ 1",
            &[5, 6],
            LineState(0),
            &[("string", "property")]
        )
        .is_ok()
    );
}

#[test]
fn resync_finds_unmatched_comment_close() {
    let lexer = LineLanguage::Json;
    assert_eq!(lexer.resync("a */ b"), Some(LineState(2)));
    assert_eq!(lexer.resync("/* a */"), Some(LineState(0)));
    assert_eq!(lexer.resync("plain"), Some(LineState(0)));
}

/// Asserts that every one-cut and two-cut split of a line matches whole-line lexing.
fn assert_all_splits(line: &str) {
    let lexer = LineLanguage::Json;
    let cuts: Vec<usize> = (0..=line.len())
        .filter(|&c| line.is_char_boundary(c))
        .collect();
    for (n, &first) in cuts.iter().enumerate() {
        let one = check_split(&lexer, line, &[first], LineState(0), &[]);
        assert!(one.is_ok(), "{line:?} at {first}: {one:?}");
        for &second in &cuts[n + 1..] {
            let two = check_split(&lexer, line, &[first, second], LineState(0), &[]);
            assert!(two.is_ok(), "{line:?} at {first}, {second}: {two:?}");
        }
    }
}

#[test]
fn malformed_numbers_split_at_every_cut_match_whole_lexing() {
    for line in [
        "1e-2.3",
        "1e+2.3",
        "1.5e3.4",
        "1.5.3",
        "-1.2.3",
        "12e",
        "0.5e-",
        "1e5.3.1",
        "1.e5",
        "--1",
        "1e+-2",
        "1e5e6",
        "12.34e-56.78",
        "[1e,-2.3]",
        "-",
        "1.",
        "1e-",
    ] {
        assert_all_splits(line);
    }
}

#[test]
fn number_tail_state_continues_across_a_cut_and_clears_at_line_end() {
    let lexer = LineLanguage::Json;
    let (spans, state) = lexer.lex("1e", LineState(0));
    assert_eq!(spans, vec![(0..1, "number")]);
    assert_eq!(
        lexer.lex("-2.3", state).0,
        vec![(0..2, "number"), (3..4, "number")]
    );
    assert_eq!(
        lexer.lex("-2.3", lexer.line_end(state)).0,
        vec![(0..4, "number")]
    );
}

#[test]
fn a_number_after_a_cut_slash_keeps_its_colour() {
    let lexer = LineLanguage::Json;
    let (spans, state) = lexer.lex("/", LineState(0));
    assert!(spans.is_empty());
    assert_eq!(lexer.lex("1", state).0, vec![(0..1, "number")]);
    assert_eq!(lexer.lex("-2.5", state).0, vec![(0..4, "number")]);
    assert_all_splits("/1");
    assert_all_splits("[/-2.5,/1e3]");
}

#[test]
fn a_minus_after_a_complete_number_cut_starts_a_new_number() {
    let lexer = LineLanguage::Json;
    let (spans, state) = lexer.lex("1", LineState(0));
    assert_eq!(spans, vec![(0..1, "number")]);
    assert_eq!(lexer.lex("-2", state).0, vec![(0..2, "number")]);
    let (_, dot) = lexer.lex("1.", LineState(0));
    assert_eq!(lexer.lex("-2", dot).0, vec![(0..2, "number")]);
    for line in ["1-2", "1.-2", "1e-2-3", "1.5-0.5e1"] {
        assert_all_splits(line);
    }
}
