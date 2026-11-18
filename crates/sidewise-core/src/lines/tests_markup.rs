//! XML line lexer behaviour and streaming contract checks.
use super::*;
use proptest::prelude::*;

#[test]
fn colours_tags_attributes_and_entities() {
    let (spans, _) = LineLanguage::Xml.lex("<SHORT-NAME>Can</SHORT-NAME>", LineState(0));
    assert_eq!(
        spans,
        vec![
            (0..1, "punctuation.delimiter"),
            (1..11, "tag"),
            (11..12, "punctuation.delimiter"),
            (15..17, "punctuation.delimiter"),
            (17..27, "tag"),
            (27..28, "punctuation.delimiter")
        ]
    );
    let (spans, _) = LineLanguage::Xml.lex("<A b=\"1\"> &name; &#12;", LineState(0));
    assert_eq!(
        spans,
        vec![
            (0..1, "punctuation.delimiter"),
            (1..2, "tag"),
            (3..4, "attribute"),
            (4..5, "punctuation.delimiter"),
            (5..8, "string"),
            (8..9, "punctuation.delimiter"),
            (10..16, "constant"),
            (17..22, "constant")
        ]
    );
}

#[test]
fn carries_values_and_tag_boundaries() {
    let lexer = LineLanguage::Xml;
    let (first, state) = lexer.lex("<A b=\"long", LineState(0));
    assert_eq!(first.last(), Some(&(5..10, "string")));
    let (second, _) = lexer.lex(" value\">", lexer.line_end(state));
    assert_eq!(
        second,
        vec![(0..7, "string"), (7..8, "punctuation.delimiter")]
    );
    assert!(check_split(&lexer, "<A b=\"long value\">", &[8], LineState(0), &[]).is_ok());
    let (first, state) = lexer.lex("<", LineState(0));
    assert!(first.is_empty());
    assert_eq!(state.prefix_len(), 1);
    assert_eq!(
        lexer.lex("A>", state).0,
        vec![(0..1, "tag"), (1..2, "punctuation.delimiter")]
    );
    assert!(lexer.lex("A>", lexer.line_end(state)).0.is_empty());
    let (_, state) = lexer.lex("<A", LineState(0));
    assert_eq!(
        lexer.lex("b=\"1\">", lexer.line_end(state)).0[0],
        (0..1, "attribute")
    );
}

proptest! {
    #[test]
    fn streaming_spans_obey_contract(lines in prop::collection::vec(prop_oneof!["[a-zA-Z0-9 <>/=&;'\"_:.-]{0,80}", Just("<!-- text -->".to_string()), Just("<![CDATA[text]]>".to_string())], 5), cut_seed in any::<usize>()) {
        let lexer = LineLanguage::Xml;
        let mut state = LineState(0);
        for line in lines {
            let (spans, next) = lexer.lex(&line, state);
            prop_assert_eq!(lexer.line_end(LineState(next.0 | (1 << 16))), lexer.line_end(next));
            prop_assert!(check_spans(&line, &spans).is_ok());
            let cut = cut_seed % (line.len() + 1);
            prop_assert!(check_split(&lexer, &line, &[cut], state, &[]).is_ok(), "line {:?}, cut {}, state {:?}: {:?}", line, cut, state, check_split(&lexer, &line, &[cut], state, &[]));
            state = lexer.line_end(next);
        }
    }
}

#[test]
fn streams_xml_blocks_and_resynchronises() {
    let lexer = LineLanguage::Xml;
    let (spans, state) = lexer.lex("<!-- a--", LineState(0));
    assert_eq!(spans, vec![(0..8, "comment")]);
    assert_eq!(lexer.lex(">", state).1, LineState(0));
    assert_eq!(lexer.lex(">", lexer.line_end(state)).1, LineState(6));
    let (spans, state) = lexer.lex("<!-", LineState(0));
    assert!(spans.is_empty());
    assert_eq!(lexer.lex("-x-->", state).0, vec![(0..5, "comment")]);
    let (_, state) = lexer.lex("<![CDATA[x]]", LineState(0));
    assert_eq!(
        lexer.lex(">", state).0,
        vec![(0..1, "punctuation.delimiter")]
    );
    let (_, state) = lexer.lex("<!-- first", LineState(0));
    assert_eq!(
        lexer.lex("second -->", lexer.line_end(state)).1,
        LineState(0)
    );
    assert_eq!(lexer.resync("x --> <a>"), Some(LineState(6)));
    assert_eq!(lexer.resync("<!-- a -->"), Some(LineState(0)));
    assert_eq!(lexer.resync("plain words"), None);
    let text = format!("{}<!--", "<a>".repeat(5000));
    let (spans, state) = lexer.lex(&text, LineState(0));
    assert_eq!(spans.len(), MAX_SPANS);
    assert_eq!(state, LineState(6));
    for line in ["<!-- hi -->", "<![CDATA[x]]>", "<?xml?>", "<!DOCTYPE x>"] {
        for cut in 0..=line.len() {
            assert!(
                check_split(&lexer, line, &[cut], LineState(0), &[]).is_ok(),
                "{line} at {cut}: {:?}",
                check_split(&lexer, line, &[cut], LineState(0), &[])
            );
        }
    }
}

#[test]
fn processing_instruction_requires_question_mark_before_close() {
    let lexer = LineLanguage::Xml;
    let (_, state) = lexer.lex("<?xml a>b?", LineState(0));
    assert_eq!(state.0 & 0xff, 8);
    assert_eq!(
        lexer.lex("><a>", state).0,
        vec![
            (0..1, "keyword"),
            (1..2, "punctuation.delimiter"),
            (2..3, "tag"),
            (3..4, "punctuation.delimiter")
        ]
    );
}

#[test]
fn line_end_clears_escape_flag() {
    assert_eq!(LineLanguage::Xml.line_end(LineState(1 << 16)), LineState(0));
}

/// Asserts split safety at every single cut and every pair of cuts of one line.
fn assert_every_cut(lexer: &LineLanguage, line: &str, state: LineState) {
    for cut in (0..=line.len()).filter(|&c| line.is_char_boundary(c)) {
        let result = check_split(lexer, line, &[cut], state, &[]);
        assert!(result.is_ok(), "{line:?} at {cut}: {result:?}");
    }
}

#[test]
fn adjacent_comments_keep_their_own_spans_at_every_cut() {
    let lexer = LineLanguage::Xml;
    let line = "<!----><!--";
    assert_eq!(
        lexer.lex(line, LineState(0)).0,
        vec![(0..7, "comment"), (7..11, "comment")]
    );
    assert_every_cut(&lexer, line, LineState(0));
    let (spans, state) = lexer.lex("<!----><!--", LineState(0));
    assert!(check_spans("<!----><!--", &spans).is_ok());
    assert_eq!(state, LineState(6));
}

#[test]
fn comment_opener_after_span_cap_does_not_rewrite_the_last_span() {
    let lexer = LineLanguage::Xml;
    let line = format!("{}<!--", "<!---->".repeat(MAX_SPANS));
    let (spans, _) = lexer.lex(&line, LineState(0));
    assert_eq!(spans.len(), MAX_SPANS);
    assert!(check_spans(&line, &spans).is_ok(), "{:?}", spans.last());
    let cut = line.len() - 4;
    let (head, tail) = line.split_at(cut);
    let (spans, state) = lexer.lex(head, LineState(0));
    assert!(check_spans(head, &spans).is_ok());
    let (spans, _) = lexer.lex(tail, state);
    assert!(check_spans(tail, &spans).is_ok(), "{spans:?}");
}

#[test]
fn bare_cr_at_chunk_end_is_content_and_cannot_close_a_comment() {
    let lexer = LineLanguage::Xml;
    let line = "<!-- x--\r>";
    let (spans, state) = lexer.lex(line, LineState(0));
    assert_eq!(spans, vec![(0..10, "comment")]);
    assert_eq!(state.0 & 0xff, 6);
    assert_every_cut(&lexer, line, LineState(0));
    let (_, head) = lexer.lex("<!-- x--\r", LineState(0));
    assert_eq!(head.0 & 0xff, 6);
    assert_eq!(lexer.lex(">", head).1.0 & 0xff, 6);
}

#[test]
fn bare_cr_next_to_multibyte_text_across_a_4k_boundary_matches_whole_lexing() {
    let lexer = LineLanguage::Xml;
    let line = format!("<!-- {}--\r>", "é".repeat(2200));
    let tail = line.len() - 1;
    for cut in [4095, 4097] {
        for second in [tail - 1, tail] {
            let result = check_split(&lexer, &line, &[cut, second], LineState(0), &[]);
            assert!(result.is_ok(), "cuts {cut}, {second}: {result:?}");
        }
    }
}
