use std::time::{Duration, Instant};

use proptest::prelude::*;

use super::*;

fn assert_well_formed(line: &str, found: &[Token]) {
    let mut end = 0;
    for token in found {
        assert!(
            token.range.start >= end,
            "overlap or out of order: {found:?}"
        );
        assert!(
            token.range.start < token.range.end,
            "empty token: {found:?}"
        );
        assert!(token.range.end <= line.len(), "past the line: {found:?}");
        assert!(line.is_char_boundary(token.range.start));
        assert!(line.is_char_boundary(token.range.end));
        end = token.range.end;
    }
}

proptest! {
    #[test]
    fn tokens_are_ordered_non_overlapping_and_on_char_boundaries(
        body in "[a-z0-9_ .xeIF\"'/*#()=+\\\\äü日本😀-]{0,60}",
        case_ignored in any::<bool>(),
    ) {
        let lang = Language { case_ignored, ..sample() };
        let line = format!("if {body}");
        let (found, _) = lex_line(&lang, &line, LineState::Normal);
        prop_assert_eq!(found.first(), Some(&Token { range: 0..2, kind: K::Keyword }));
        assert_well_formed(&line, &found);
    }
}

#[test]
fn one_megabyte_line_lexes_in_under_a_second() {
    let chunk = r#"if gift 42 + "s\"t" (* c *) "#;
    let count = (1 << 20) / chunk.len() + 1;
    let line = chunk.repeat(count);
    let started = Instant::now();
    let (found, _) = lex_line(&sample(), &line, LineState::Normal);
    let elapsed = started.elapsed();
    assert_eq!(found.len(), count * 5);
    assert!(elapsed < Duration::from_secs(1), "took {elapsed:?}");
}
