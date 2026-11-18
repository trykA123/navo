//! Contract checks for streaming line lexers.
use super::*;

#[test]
fn language_names_are_exact() {
    for (name, expected) in [
        ("xml", Some(LineLanguage::Xml)),
        ("json", Some(LineLanguage::Json)),
        ("jsonc", Some(LineLanguage::Json)),
        ("json5", Some(LineLanguage::Json)),
        ("log", Some(LineLanguage::Log)),
        ("text", Some(LineLanguage::Log)),
        ("XML", None),
    ] {
        assert_eq!(LineLanguage::for_language(name), expected);
    }
}

#[test]
fn span_contract_rejects_invalid_spans() {
    assert!(check_spans("abc", &[(0..2, "comment"), (1..3, "string")]).is_err());
    assert!(check_spans("é", &[(0..1, "comment")]).is_err());
    assert!(check_spans("abc", &[(0..1, "unknown")]).is_err());
    assert!(check_spans("abc", &[(0..1, "comment"), (2..3, "string")]).is_ok());
}

struct Fake(u8);
impl LineLexer for Fake {
    fn name(&self) -> &str {
        "fake"
    }
    fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        match self.0 {
            0 => (
                vec![(
                    0..chunk.len(),
                    if chunk.len() == 4 {
                        "comment"
                    } else {
                        "string"
                    },
                )],
                state,
            ),
            1 if chunk == "<!-" => (vec![], LineState(3 << 8)),
            1 => (vec![(0..chunk.len(), "comment")], state),
            2 => (
                vec![(
                    0..chunk.len(),
                    if chunk.len() == 4 {
                        "property"
                    } else {
                        "string"
                    },
                )],
                state,
            ),
            _ => (
                (0..chunk.len().min(MAX_SPANS))
                    .map(|i| (i..i + 1, "comment"))
                    .collect(),
                state,
            ),
        }
    }
}

#[test]
fn split_detects_wrong_comment_colour() {
    assert!(check_split(&Fake(0), "abcd", &[2], LineState(0), &[]).is_err());
}
#[test]
fn split_exempts_carried_prefix() {
    assert!(check_split(&Fake(1), "<!--", &[3], LineState(0), &[]).is_ok());
}
#[test]
fn split_accepts_explicit_role_pair() {
    assert!(check_split(&Fake(2), "abcd", &[2], LineState(0), &[]).is_err());
    assert!(
        check_split(
            &Fake(2),
            "abcd",
            &[2],
            LineState(0),
            &[("string", "property")]
        )
        .is_ok()
    );
}
#[test]
fn split_exempts_bytes_after_cap_horizon() {
    let line = "a".repeat(MAX_SPANS + 10);
    assert!(check_split(&Fake(3), &line, &[MAX_SPANS + 1], LineState(0), &[]).is_ok());
}
#[test]
fn line_end_clears_only_delimiter_bits() {
    assert_eq!(
        Fake(0).line_end(LineState(0x1234_ffff)),
        LineState(0x1234_00ff)
    );
}

/// Colours a chunk as a comment only when it ends the line.
struct EndOnly;
impl LineLexer for EndOnly {
    fn name(&self) -> &str {
        "end-only"
    }
    fn lex(&self, _chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        (Vec::new(), state)
    }
    fn lex_last(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        (vec![(0..chunk.len(), "comment")], state)
    }
}

#[test]
fn line_end_mode_lexes_the_whole_line_and_last_chunk_with_lex_last() {
    assert!(check_split(&EndOnly, "abcd", &[2], LineState(0), &[]).is_ok());
    assert!(check_split_line_end(&EndOnly, "abcd", &[2], LineState(0), &[]).is_err());
    assert!(check_split_line_end(&EndOnly, "abcd", &[], LineState(0), &[]).is_ok());
}
