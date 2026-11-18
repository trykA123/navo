//! Log lexer chunk-cut tests: carried skip and quote state, and D7 split-safety proptests.
use super::*;
use proptest::prelude::*;

/// Lexes chunks in order with carried state; `last` makes the final chunk end its line.
fn run(chunks: &[&str], last: bool) -> Vec<Vec<Span>> {
    let lexer = LineLanguage::Log;
    let mut state = LineState(0);
    let mut out = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        let (spans, next) = if last && index + 1 == chunks.len() {
            lexer.lex_last(chunk, state)
        } else {
            lexer.lex(chunk, state)
        };
        out.push(spans);
        state = next;
    }
    out
}

/// Checks one cut of a line against whole-line lexing with the default accept list,
/// in plain mode and in line-end mode (`lex_last` on the whole line and last chunk).
fn assert_cut(line: &str, cuts: &[usize]) {
    let split = check_split(&LineLanguage::Log, line, cuts, LineState(0), &[]);
    assert!(split.is_ok(), "{line:?} at {cuts:?}: {split:?}");
    let ended = check_split_line_end(&LineLanguage::Log, line, cuts, LineState(0), &[]);
    assert!(ended.is_ok(), "{line:?} at {cuts:?} (line end): {ended:?}");
}

#[test]
fn a_number_cut_from_its_unit_gives_no_span() {
    assert!(run(&["12", "ms x"], false).concat().is_empty());
    assert!(run(&["12", "ms"], true).concat().is_empty());
    assert_cut("12ms x", &[2]);
}

#[test]
fn a_date_cut_inside_gives_no_span_on_either_side() {
    assert!(run(&["2026", "/11/18 x"], false).concat().is_empty());
    assert!(run(&["2026", "/11/18"], true).concat().is_empty());
    assert_cut("2026/11/18 x", &[4]);
}

#[test]
fn a_single_quote_cut_loses_its_colour_and_invents_none() {
    assert!(run(&["'ab", "cd'"], false).concat().is_empty());
    let last = run(&["'ab", "cd'"], true);
    assert!(last[0].is_empty());
    assert_eq!(last[1], vec![(0..3, "string.special")]);
    assert_cut("'abcd' x", &[3]);
}

#[test]
fn a_single_quote_cut_before_a_word_byte_is_no_string() {
    assert!(run(&["'a'", "b"], false).concat().is_empty());
    assert!(run(&["'a'", "b"], true).concat().is_empty());
    assert_cut("'a'b x", &[3]);
}

#[test]
fn a_single_quote_cut_before_a_boundary_closes_without_a_span() {
    assert!(run(&["'a'", " x"], true).concat().is_empty());
    assert_cut("'a' x", &[3]);
}

#[test]
fn a_level_word_cut_in_two_gives_no_span() {
    assert!(run(&["DEB", "UG x"], false).concat().is_empty());
    assert_cut("DEBUG x", &[3]);
    assert_cut("ERROR: x", &[2]);
}

#[test]
fn a_double_string_after_a_cut_token_keeps_exact_coverage() {
    let chunks = run(&["ab", "c=\"foo bar\" x"], false);
    assert!(chunks[0].is_empty());
    assert_eq!(chunks[1], vec![(2..11, "string")]);
    assert_cut("abc=\"foo bar\" x", &[2]);
    assert_cut("C:\\a\\\"foo bar\" x", &[5]);
}

#[test]
fn a_single_string_never_swallows_a_double_string() {
    assert_cut("ab='x \"foo bar\" y' z", &[5]);
    assert_cut("'x \"foo bar\" y' z", &[3]);
}

#[test]
fn every_cut_of_representative_lines_matches_whole_lexing() {
    for line in [
        "[ERROR] x",
        "error: x",
        "WARN: v1.2.3 file.txt",
        "Error] Info| notice: x",
        "\"a \\\"quoted\\\" word\" tail",
        "na\u{ef}ve ERROR",
        "a.ERROR ERROR.LOG",
        "ERROR_LINE DBG",
        "na\u{ef}ve 42 \u{e9}42",
        "0x1F x",
        "1.2.3 x",
        "x -5 7 a-5",
        "1e5 1e+5 x",
        "01234567 1234567890ABCDEF x",
        "2026-11-18T09:15:02.123Z INFO x",
        "18.11.2026 09:15 x",
        "see https://a.b/c.d?q=\"x y\" and C:\\a\\b\"c d\" tail",
        "x 'now' 'a b' c'd' e 'f",
        "a='x y ERROR z' DEBUG 'a\"b c\"d' e",
        "'a 'b' c' 'one two three' x",
        "{6F9619FF-8B86-D011-B42D-00C04FC964FF} \\\\srv\\share /usr/lib/x.so y",
    ] {
        let cuts: Vec<usize> = (0..=line.len())
            .filter(|&cut| line.is_char_boundary(cut))
            .collect();
        for (index, &first) in cuts.iter().enumerate() {
            assert_cut(line, &[first]);
            for &second in &cuts[index + 1..] {
                assert_cut(line, &[first, second]);
            }
        }
    }
}

#[test]
fn cuts_near_four_kib_match_whole_lexing() {
    let unit = "ERROR 09:15:02 0x1F \"quoted text\" C:\\a\\b 'ab cd' 2026-11-18T01:02:03Z /usr/x/y abcdef.ghi 12ms ";
    let line = unit.repeat(8192 / unit.len() + 1);
    for cut in 4080..4112 {
        assert_cut(&line, &[cut]);
    }
    assert_cut(&line, &[4096, 8192]);
}

/// Builds a line from tokens glued to letters and to each other by mixed separators.
fn glued_line() -> impl Strategy<Value = String> {
    let token = prop_oneof![
        "[a-zA-Z_]{1,8}".boxed(),
        "[a-zA-Z_]{1,4}(\\.[a-zA-Z_]{1,4})*".boxed(),
        "-?[0-9]{1,4}(\\.[0-9]{1,3})*([eE][+-]?[0-9]{1,3})?[a-z]{0,2}".boxed(),
        "0[xX][0-9A-Fa-f]{1,6}".boxed(),
        "[0-9A-Fa-f]{8}".boxed(),
        "[0-9A-Fa-f]{16}".boxed(),
        "[0-9]{4}[-/.][0-9]{2}[-/.][0-9]{2}(T[0-9]{2}:[0-9]{2}(:[0-9]{2}([.,][0-9]{1,3})?)?Z?)?"
            .boxed(),
        "[0-9]{1,2}:[0-9]{2}(:[0-9]{2})?".boxed(),
        "https?://[a-z./?=&]{1,12}".boxed(),
        "[A-Z]:[\\\\/][a-z\\\\/.]{1,8}".boxed(),
        "/[a-z]{1,4}/[a-z/.]{1,6}".boxed(),
        "\\{?[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\\}?".boxed(),
        "(ERROR|WARN|info|Debug|DBG|trace|FATAL)[:|\\]]?".boxed(),
        "\"[a-zA-Z0-9 _/:'.\\\\-]{0,10}\"".boxed(),
        "'[a-zA-Z0-9 _/:.-]{0,12}'".boxed(),
    ];
    prop::collection::vec((token, "[ ,|/:'.\\[\\]()<>=\u{e9}-]{0,2}"), 1..12)
        .prop_map(|pairs| pairs.into_iter().map(|(t, s)| format!("{t}{s}")).collect())
}

/// Turns seeds into strictly rising cuts on character boundaries inside the line.
fn cuts_from(line: &str, seeds: &[usize]) -> Vec<usize> {
    let mut cuts: Vec<usize> = seeds.iter().map(|seed| seed % (line.len() + 1)).collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts.retain(|&cut| line.is_char_boundary(cut));
    cuts
}

proptest! {
    /// Streams random lines from random states and checks the span contract.
    #[test]
    fn streaming_spans_obey_contract(
        lines in prop::collection::vec("[a-zA-Z0-9 {}()\\[\\]<>:;,.|/\\\\\"'_-]{0,80}", 5),
    ) {
        let lexer = LineLanguage::Log;
        let mut state = LineState(0);
        for line in lines {
            for last in [false, true] {
                let (spans, next) = if last { lexer.lex_last(&line, state) } else { lexer.lex(&line, state) };
                prop_assert!(check_spans(&line, &spans).is_ok(), "line {:?}", line);
                if !last { state = lexer.line_end(next); }
            }
        }
    }

    /// Cuts glued token lines at random points; the default accept list must hold.
    #[test]
    fn glued_tokens_split_like_whole_lines(
        line in glued_line().no_shrink(),
        seeds in prop::collection::vec(any::<usize>(), 1..4),
    ) {
        let cuts = cuts_from(&line, &seeds);
        let split = check_split(&LineLanguage::Log, &line, &cuts, LineState(0), &[]);
        prop_assert!(split.is_ok(), "line {:?}, cuts {:?}: {:?}", line, cuts, split);
        let ended = check_split_line_end(&LineLanguage::Log, &line, &cuts, LineState(0), &[]);
        prop_assert!(ended.is_ok(), "line {:?}, cuts {:?} (line end): {:?}", line, cuts, ended);
    }

    /// Cuts lines of raw separator soup, which glues and quotes at random.
    #[test]
    fn raw_soup_splits_like_whole_lines(
        line in "[a-zA-Z0-9 {}()\\[\\]<>:;,.|/\\\\\"'_=\u{e9}-]{0,160}",
        seeds in prop::collection::vec(any::<usize>(), 1..4),
    ) {
        let cuts = cuts_from(&line, &seeds);
        let split = check_split(&LineLanguage::Log, &line, &cuts, LineState(0), &[]);
        prop_assert!(split.is_ok(), "line {:?}, cuts {:?}: {:?}", line, cuts, split);
        let ended = check_split_line_end(&LineLanguage::Log, &line, &cuts, LineState(0), &[]);
        prop_assert!(ended.is_ok(), "line {:?}, cuts {:?} (line end): {:?}", line, cuts, ended);
    }

    /// Cuts long glued lines near the 4 KiB row size.
    #[test]
    fn long_glued_lines_split_at_row_size(
        parts in prop::collection::vec(glued_line().no_shrink(), 40..60),
        delta in 0usize..64,
    ) {
        let line: String = parts.concat();
        prop_assume!(line.len() > 4200);
        let cut = (4096 - delta).min(line.len());
        let cuts = cuts_from(&line, &[cut]);
        let split = check_split(&LineLanguage::Log, &line, &cuts, LineState(0), &[]);
        prop_assert!(split.is_ok(), "cuts {:?}: {:?}", cuts, split);
        let ended = check_split_line_end(&LineLanguage::Log, &line, &cuts, LineState(0), &[]);
        prop_assert!(ended.is_ok(), "cuts {:?} (line end): {:?}", cuts, ended);
    }
}
