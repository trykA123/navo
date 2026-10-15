use super::*;

#[test]
fn keywords_match_whole_words_only() {
    let line = "if gift if_x xif äif 日本if if";
    assert_eq!(
        tokens(&sample(), line),
        [("if", K::Keyword), ("if", K::Keyword)]
    );
}

#[test]
fn keywords_are_case_sensitive_by_default() {
    assert_eq!(tokens(&sample(), "IF If if"), [("if", K::Keyword)]);
}

#[test]
fn case_ignored_keywords_match_any_case() {
    let lang = Language {
        case_ignored: true,
        ..sample()
    };
    assert_eq!(
        tokens(&lang, "IF If iF"),
        [("IF", K::Keyword), ("If", K::Keyword), ("iF", K::Keyword)]
    );
}

#[test]
fn each_keyword_group_gives_its_own_kind() {
    assert_eq!(
        tokens(&sample(), "while TRUE true"),
        [("while", K::Keyword), ("TRUE", K::Constant)]
    );
}

#[test]
fn prefix_groups_match_words_that_start_with_a_keyword() {
    assert_eq!(
        tokens(&sample(), "dsGetClass dsGet xdsGet dsget"),
        [("dsGetClass", K::Function), ("dsGet", K::Function)]
    );
}

#[test]
fn line_comment_runs_to_the_end_of_the_line() {
    assert_eq!(
        spans(&sample(), "x = 1 # note if", LineState::Normal),
        (
            vec![
                ("=", K::Operator),
                ("1", K::Number),
                ("# note if", K::Comment)
            ],
            LineState::Normal
        )
    );
}

#[test]
fn block_comment_closed_on_the_same_line() {
    assert_eq!(
        spans(&sample(), "a /* b */ if", LineState::Normal),
        (
            vec![("/* b */", K::Comment), ("if", K::Keyword)],
            LineState::Normal
        )
    );
}

#[test]
fn block_comment_spans_three_lines_with_state_carried() {
    let lang = sample();
    let first = spans(&lang, "x (* one", LineState::Normal);
    assert_eq!(first, (vec![("(* one", K::Comment)], LineState::Block(1)));
    let second = spans(&lang, "two // still \"in\"", first.1);
    assert_eq!(
        second,
        (
            vec![("two // still \"in\"", K::Comment)],
            LineState::Block(1)
        )
    );
    let third = spans(&lang, "three *) if", second.1);
    assert_eq!(
        third,
        (
            vec![("three *)", K::Comment), ("if", K::Keyword)],
            LineState::Normal
        )
    );
}

#[test]
fn empty_line_keeps_an_open_block_comment() {
    assert_eq!(
        spans(&sample(), "", LineState::Block(0)),
        (vec![], LineState::Block(0))
    );
}

#[test]
fn escape_before_the_close_does_not_close_the_delimiter() {
    assert_eq!(
        tokens(&sample(), r#"s = "a\"b" + 1"#),
        [
            ("=", K::Operator),
            (r#""a\"b""#, K::String),
            ("+", K::Operator),
            ("1", K::Number)
        ]
    );
}

#[test]
fn escaped_escape_still_closes_the_delimiter() {
    assert_eq!(
        tokens(&sample(), r#""a\\" if"#),
        [(r#""a\\""#, K::String), ("if", K::Keyword)]
    );
}

#[test]
fn unterminated_single_line_delimiter_ends_at_the_line_end() {
    assert_eq!(
        spans(&sample(), r#"say "open // not comment"#, LineState::Normal),
        (
            vec![(r#""open // not comment"#, K::String)],
            LineState::Normal
        )
    );
}

#[test]
fn multiline_delimiter_carries_over_lines() {
    let lang = sample();
    let first = spans(&lang, "t = '''start", LineState::Normal);
    assert_eq!(
        first,
        (
            vec![("=", K::Operator), ("'''start", K::String)],
            LineState::Delimited(1)
        )
    );
    let second = spans(&lang, "middle # if", first.1);
    assert_eq!(
        second,
        (vec![("middle # if", K::String)], LineState::Delimited(1))
    );
    let third = spans(&lang, "end''' if", second.1);
    assert_eq!(
        third,
        (
            vec![("end'''", K::String), ("if", K::Keyword)],
            LineState::Normal
        )
    );
}

#[test]
fn number_forms() {
    assert_eq!(
        tokens(
            &sample(),
            "42 0x1F 0b101 3.14 1e9 2.5e-3 10u 7l 1.5f x1 y_2"
        ),
        [
            ("42", K::Number),
            ("0x1F", K::Number),
            ("0b101", K::Number),
            ("3.14", K::Number),
            ("1e9", K::Number),
            ("2.5e-3", K::Number),
            ("10u", K::Number),
            ("7l", K::Number),
            ("1.5f", K::Number)
        ]
    );
}

#[test]
fn operators_take_the_longest_match() {
    assert_eq!(
        tokens(&sample(), "a == b->c + d / e - f"),
        [
            ("==", K::Operator),
            ("->", K::Operator),
            ("+", K::Operator),
            ("/", K::Operator)
        ]
    );
}

#[test]
fn line_comment_inside_a_string_is_not_a_comment() {
    assert_eq!(
        tokens(&sample(), r#"x = "a // b" // c"#),
        [
            ("=", K::Operator),
            (r#""a // b""#, K::String),
            ("// c", K::Comment)
        ]
    );
}

#[test]
fn quote_inside_a_comment_is_not_a_string() {
    assert_eq!(
        tokens(&sample(), r#"/* "q */ "r""#),
        [(r#"/* "q */"#, K::Comment), (r#""r""#, K::String)]
    );
}

#[test]
fn line_comment_beats_an_operator_that_prefixes_it() {
    assert_eq!(
        tokens(&sample(), "a / b // c"),
        [("/", K::Operator), ("// c", K::Comment)]
    );
}

#[test]
fn non_ascii_text_gives_char_boundary_ranges() {
    let line = "ä if 日本 \"😀\" + 1 // ü";
    let (found, _) = lex_line(&sample(), line, LineState::Normal);
    assert!(
        found
            .iter()
            .all(|t| line.is_char_boundary(t.range.start) && line.is_char_boundary(t.range.end))
    );
    assert_eq!(
        tokens(&sample(), line),
        [
            ("if", K::Keyword),
            ("\"😀\"", K::String),
            ("+", K::Operator),
            ("1", K::Number),
            ("// ü", K::Comment)
        ]
    );
}

fn lilypond() -> Language {
    Language {
        line_comments: owned(&["%"]),
        block_comments: vec![("%{".to_owned(), "%}".to_owned())],
        ..sample()
    }
}

#[test]
fn longer_block_opener_beats_a_line_comment_it_starts_with() {
    assert_eq!(
        spans(&lilypond(), "a %{ b %} if % c", LineState::Normal),
        (
            vec![
                ("%{ b %}", K::Comment),
                ("if", K::Keyword),
                ("% c", K::Comment)
            ],
            LineState::Normal
        )
    );
    assert_eq!(
        spans(&lilypond(), "x %{ open", LineState::Normal),
        (vec![("%{ open", K::Comment)], LineState::Block(0))
    );
}

fn many_delimiters(count: usize, multiline: usize) -> Language {
    let delimiters = (0..count)
        .map(|i| delimiter(&format!("<d{i}>"), "</d>", None, i == multiline))
        .collect();
    Language {
        delimiters,
        ..sample()
    }
}

#[test]
fn delimiter_past_index_255_carries_to_the_next_line() {
    let lang = many_delimiters(300, 280);
    let first = spans(&lang, "x <d280>open", LineState::Normal);
    assert_eq!(
        first,
        (vec![("<d280>open", K::String)], LineState::Delimited(280))
    );
    assert_eq!(
        spans(&lang, "still</d> if", first.1),
        (
            vec![("still</d>", K::String), ("if", K::Keyword)],
            LineState::Normal
        )
    );
}

#[test]
fn delimiter_past_the_u16_range_does_not_carry() {
    let last = usize::from(u16::MAX) + 1;
    let lang = many_delimiters(last + 1, last);
    let line = format!("<d{last}>open");
    let (found, next) = lex_line(&lang, &line, LineState::Normal);
    assert_eq!(found.len(), 1);
    assert_eq!(next, LineState::Normal);
}
