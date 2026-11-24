use crate::find::{FindQuery, TokenKind};

fn parsed(input: &str) -> FindQuery {
    let query = FindQuery::parse(input);
    assert!(
        query.tokens.iter().all(|token| token.message.is_none()),
        "{input}: {:?}",
        query.tokens
    );
    query
}

fn error(input: &str) -> String {
    FindQuery::parse(input).tokens[0]
        .message
        .clone()
        .unwrap_or_default()
}

#[test]
fn the_new_keys_have_their_own_token_kinds() {
    for (input, kind) in [
        ("name:*.rs", TokenKind::Name),
        ("depth:1", TokenKind::Depth),
        ("word:on", TokenKind::Word),
        ("not:on", TokenKind::Not),
        ("case:on", TokenKind::Case),
    ] {
        assert_eq!(parsed(input).tokens[0].kind, kind, "{input}");
    }
}

#[test]
fn name_keeps_the_raw_mask() {
    let query = parsed("needle name:*.rs;!test*");
    assert_eq!(query.name.as_deref(), Some("*.rs;!test*"));
    assert_eq!(query.text, "needle");
}

#[test]
fn name_accepts_a_case_insensitive_regex() {
    assert_eq!(
        parsed("name:/^lib.*[.]rs$/").name.as_deref(),
        Some("/^lib.*[.]rs$/")
    );
}

#[test]
fn name_rejects_an_empty_mask_or_part() {
    for input in ["name:", "name:*.rs;", "name:!"] {
        assert_eq!(error(input), "name: use masks like *.rs;!test* or /regex/");
    }
}

#[test]
fn name_rejects_an_invalid_regex() {
    assert_eq!(
        error("name:/(/"),
        "name: use masks like *.rs;!test* or /regex/"
    );
}

#[test]
fn depth_accepts_zero_to_sixty_four() {
    assert_eq!(parsed("depth:0").depth, Some(0));
    assert_eq!(parsed("depth:64").depth, Some(64));
}

#[test]
fn depth_rejects_out_of_range_and_text() {
    for input in ["depth:65", "depth:x", "depth:-1"] {
        assert_eq!(error(input), "depth: use a number from 0 to 64");
    }
}

#[test]
fn word_not_and_case_toggle_on_and_off() {
    let query = parsed("word:on not:on case:on");
    assert!(query.word);
    assert!(query.invert);
    assert!(query.case);

    let query = parsed("word:off not:off case:off");
    assert!(!query.word);
    assert!(!query.invert);
    assert!(!query.case);
}

#[test]
fn toggles_reject_other_values() {
    for (input, key) in [("word:yes", "word"), ("not:x", "not"), ("case:1", "case")] {
        assert_eq!(error(input), format!("{key}: use on or off"));
    }
}
