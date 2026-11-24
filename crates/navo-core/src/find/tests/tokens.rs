use crate::find::{FindQuery, TextEncoding, TokenError, TokenKind};

fn parsed(input: &str) -> FindQuery {
    let query = FindQuery::parse(input);
    assert!(
        query.tokens.iter().all(|token| token.message.is_none()),
        "{input}: {:?}",
        query.tokens
    );
    query
}

fn error(input: &str) -> TokenError {
    FindQuery::parse(input).tokens[0]
        .message
        .clone()
        .expect("a token error")
}

#[test]
fn the_new_keys_have_their_own_token_kinds() {
    for (input, kind) in [
        ("name:*.rs", TokenKind::Name),
        ("depth:1", TokenKind::Depth),
        ("word:on", TokenKind::Word),
        ("not:on", TokenKind::Not),
        ("case:on", TokenKind::Case),
        ("hex:4d5a", TokenKind::Hex),
        ("enc:auto", TokenKind::Enc),
        ("docs:on", TokenKind::Docs),
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
        assert_eq!(error(input), TokenError::Name);
    }
}

#[test]
fn name_rejects_an_invalid_regex() {
    assert_eq!(error("name:/(/"), TokenError::Name);
}

#[test]
fn depth_accepts_zero_to_sixty_four() {
    assert_eq!(parsed("depth:0").depth, Some(0));
    assert_eq!(parsed("depth:64").depth, Some(64));
}

#[test]
fn depth_rejects_out_of_range_and_text() {
    for input in ["depth:65", "depth:x", "depth:-1"] {
        assert_eq!(error(input), TokenError::Depth { max: 64 });
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
        assert_eq!(error(input), TokenError::Toggle { key });
    }
}

#[test]
fn hex_drops_separators_and_keeps_bytes() {
    assert_eq!(
        parsed("hex:\"4D 5A-cd:EF\"").hex,
        Some(vec![0x4d, 0x5a, 0xcd, 0xef])
    );
    assert_eq!(parsed("hex:00ff").hex, Some(vec![0x00, 0xff]));
}

#[test]
fn hex_rejects_bad_pairs_and_size() {
    for input in ["hex:", "hex:4", "hex:zz", "hex:\"4D 5G\""] {
        assert_eq!(error(input), TokenError::Hex { max: 256 });
    }
    let too_long = format!("hex:{}", "ab".repeat(257));
    assert_eq!(error(&too_long), TokenError::Hex { max: 256 });
}

#[test]
fn hex_accepts_the_full_256_bytes() {
    let input = format!("hex:{}", "ab".repeat(256));
    assert_eq!(parsed(&input).hex.map(|bytes| bytes.len()), Some(256));
}

#[test]
fn enc_keeps_the_named_list_in_order() {
    assert_eq!(
        parsed("enc:utf8,auto,utf16").encodings,
        [TextEncoding::Utf8, TextEncoding::Auto, TextEncoding::Utf16]
    );
}

#[test]
fn enc_rejects_unknown_empty_and_duplicate_names() {
    for input in ["enc:", "enc:utf8,", "enc:latin", "enc:auto,auto"] {
        assert_eq!(error(input), TokenError::Encoding);
    }
}

#[test]
fn docs_toggles_on_and_off() {
    assert!(parsed("docs:on").docs);
    assert!(!parsed("docs:off").docs);
    assert_eq!(error("docs:yes"), TokenError::Toggle { key: "docs" });
}

#[test]
fn depth_rejects_a_leading_plus() {
    assert_eq!(error("depth:+5"), TokenError::Depth { max: 64 });
}

#[test]
fn hex_rejects_a_leading_plus() {
    assert_eq!(error("hex:+f"), TokenError::Hex { max: 256 });
}

#[test]
fn the_last_repeated_token_wins() {
    assert_eq!(parsed("depth:1 depth:2").depth, Some(2));
    assert_eq!(parsed("enc:utf8 enc:auto").encodings, [TextEncoding::Auto]);
    assert_eq!(parsed("hex:aa hex:bb").hex, Some(vec![0xbb]));
}

#[test]
fn token_errors_are_typed_variants() {
    assert_eq!(error("depth:65"), TokenError::Depth { max: 64 });
    assert_eq!(error("name:"), TokenError::Name);
    assert_eq!(error("word:x"), TokenError::Toggle { key: "word" });
}
