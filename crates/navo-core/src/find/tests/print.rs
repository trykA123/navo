use crate::find::FindQuery;
use proptest::prelude::*;

/// Prints and reads back: `parse(to_text(q))` equals `q` without its token spans.
fn assert_round_trip(input: &str) {
    let mut expected = FindQuery::parse(input);
    expected.tokens.clear();
    let printed = expected.to_text();
    let mut parsed = FindQuery::parse(&printed);
    parsed.tokens.clear();
    assert_eq!(parsed, expected, "printed: {printed}");
}

#[test]
fn empty_query_prints_nothing() {
    assert_eq!(FindQuery::default().to_text(), "");
}

#[test]
fn every_filter_round_trips() {
    for input in [
        "needle",
        "hello \"two words\" world",
        r"re:foo\s+bar",
        r#"re:"foo bar""#,
        "needle ext:c,h,map",
        "size:<50MB",
        "size:>1k",
        "size:1k..2M",
        "mod:<7d",
        "mod:>7d",
        "mod:<2024-02-29",
        "mod:>2026-10-01",
        "mod:today",
        "attr:hidden,readonly,system,archive,-hidden",
        "-hidden",
        "in:src",
        "in:/tmp",
        r"in:C:\work",
        r"in:\\host\share",
        "in:\"my folder\"",
        "in:src in:lib",
        "needle name:*.rs;!test*",
        "name:/^lib.*[.]rs$/",
        r"name:/C:\lib/",
        "depth:0",
        "depth:64",
        "word:on",
        "not:on",
        "case:on",
        "needle git:off name:*.rs depth:2 word:on not:on case:on",
        "hex:4d5a",
        "hex:\"4D 5A-CD:EF\"",
        "enc:auto,ansi,dos,utf8,utf16",
        "docs:on",
        "docs:off",
        "needle hex:4d5a enc:utf8,auto docs:on",
        "arc:on",
        "arc:off",
        "git:off",
        "needle GIT:off git:on",
        "needle ext:rs size:<1M mod:<7d attr:hidden in:/tmp arc:on git:off",
    ] {
        assert_round_trip(input);
    }
}

#[test]
fn filters_print_in_canonical_order_with_plain_byte_counts() {
    let query = FindQuery::parse("git:off arc:on -hidden mod:<7d size:<1M ext:rs in:/tmp needle");
    assert_eq!(
        query.to_text(),
        "needle ext:rs size:<1048576 mod:<7d attr:-hidden in:/tmp arc:on git:off"
    );
}

#[test]
fn every_in_entry_prints_in_order() {
    let query = FindQuery::parse("in:src in:\"my folder\"");
    assert_eq!(query.to_text(), "in:src in:\"my folder\"");
}

#[test]
fn new_token_kinds_print_after_git() {
    let query = FindQuery::parse("needle git:off case:on not:on word:on depth:2 name:*.rs");
    assert_eq!(
        query.to_text(),
        "needle git:off name:*.rs depth:2 word:on not:on case:on"
    );
}

#[test]
fn hex_prints_lowercase_pairs_without_separators() {
    let query = FindQuery::parse("hex:\"4D 5A-CD:EF\"");
    assert_eq!(query.to_text(), "hex:4d5acdef");
}

#[test]
fn encodings_print_in_their_parsed_order() {
    let query = FindQuery::parse("enc:utf8,auto,utf16");
    assert_eq!(query.to_text(), "enc:utf8,auto,utf16");
}

#[test]
fn typed_tokens_print_after_case() {
    let query = FindQuery::parse("needle case:on docs:on enc:utf8 hex:4d5a");
    assert_eq!(query.to_text(), "needle case:on hex:4d5a enc:utf8 docs:on");
}

#[test]
fn regex_text_keeps_its_prefix_and_quotes_spaces() {
    let query = FindQuery::parse(r#"re:"foo bar""#);
    assert_eq!(query.to_text(), r#"re:"foo bar""#);
}

#[test]
fn in_path_with_spaces_prints_quoted() {
    let query = FindQuery::parse(r#"in:"C:\My files""#);
    assert_eq!(query.to_text(), r#"in:"C:\My files""#);
}

#[test]
fn text_that_looks_like_a_token_reprints_as_text() {
    for input in [r#""ext:rs""#, r#""-hidden""#, r#""in:x""#] {
        assert_round_trip(input);
        assert_eq!(FindQuery::parse(input).to_text(), input);
    }
}

#[test]
fn runs_of_spaces_in_text_survive_reprinting() {
    let query = FindQuery::parse(r#""two  spaces""#);
    assert_round_trip(r#""two  spaces""#);
    assert_eq!(query.text, "two  spaces");
    assert_eq!(query.to_text(), r#""two  spaces""#);
}

#[test]
fn extension_lists_with_spaces_quote_their_value() {
    let query = FindQuery::parse(r#"ext:"foo bar,rs""#);
    assert_eq!(query.ext, ["foo bar", "rs"]);
    assert_eq!(query.to_text(), r#"ext:"foo bar,rs""#);
    assert_round_trip(r#"ext:"foo bar,rs""#);
}

#[test]
fn regex_text_with_spaces_round_trips() {
    assert_round_trip(r#"re:"a b""#);
    assert_eq!(FindQuery::parse(r#"re:"a b""#).to_text(), r#"re:"a b""#);
}

#[test]
fn literal_quotes_in_text_print_escaped() {
    let query = FindQuery::parse(r#""he said \"hi\"""#);
    assert_eq!(query.text, r#"he said "hi""#);
    assert_eq!(query.to_text(), r#""he said \"hi\"""#);
}

#[test]
fn literal_quotes_round_trip_in_text_regex_and_paths() {
    for input in [
        r#""he said \"hi\"""#,
        r#"re:"a \"b\"""#,
        r#"in:"C:\My \"files\"""#,
    ] {
        assert_round_trip(input);
    }
}

#[test]
fn a_path_can_end_with_a_backslash() {
    let query = FindQuery::parse(r#"in:"C:\My files\""#);
    assert_eq!(query.within, [r"C:\My files\"]);
    assert_eq!(query.to_text(), r#"in:"C:\My files\""#);
    assert_round_trip(r#"in:"C:\My files\""#);
}

#[test]
fn a_quoted_folder_ending_in_a_backslash_closes_its_token() {
    for input in [r#"in:"C:\" in:"D:\""#, r#"in:"C:\My files\" ext:"foo bar""#] {
        let query = FindQuery::parse(input);
        assert!(
            query.tokens.iter().all(|token| token.message.is_none()),
            "{input}: {:?}",
            query.tokens
        );
        assert_round_trip(input);
    }
    assert_eq!(
        FindQuery::parse(r#"in:"C:\" in:"D:\""#).within,
        [r"C:\", r"D:\"]
    );
}

#[test]
fn a_path_ending_in_a_backslash_is_a_clean_last_token() {
    let input = r#"in:"C:\My files\""#;
    let query = FindQuery::parse(input);
    assert!(query.tokens.iter().all(|token| token.message.is_none()));
    assert_eq!(query.within, [r"C:\My files\"]);
    assert_round_trip(input);
}

#[test]
fn a_real_escaped_quote_inside_text_round_trips() {
    let input = r#""he said \"hi\"" again"#;
    assert_round_trip(input);
    assert_eq!(FindQuery::parse(input).to_text(), input);
}

#[test]
fn regex_text_with_a_quote_before_a_space_round_trips() {
    assert_round_trip(r#"re:"a \"b\"" c"#);
}

#[test]
fn a_failed_regex_leaves_the_query_unchanged() {
    let query = FindQuery::parse("[ re:a");
    assert!(query.tokens[1].message.is_some());
    assert!(!query.regex);
    assert_round_trip("[ re:a");
}

#[test]
fn quoted_segments_keep_every_space_between_them() {
    for input in [
        r#""a\"" " b""#,
        r#""a\"" " ""#,
        r#"re:"a\"" " b""#,
        r#"re:"a\"" " ""#,
    ] {
        assert_round_trip(input);
    }
}

fn text_pieces() -> impl Strategy<Value = String> {
    (
        0..4u8,
        proptest::collection::vec(
            prop_oneof![
                Just('a'),
                Just('\\'),
                Just('"'),
                Just(' '),
                Just('\t'),
                Just('['),
                Just(']'),
            ],
            0..32,
        ),
    )
        .prop_map(|(prefix, chars)| {
            let mut text: String = chars.into_iter().collect();
            match prefix {
                1 => text.insert_str(0, "re:"),
                2 => text.insert_str(0, "name:"),
                3 => text.insert_str(0, "in:"),
                _ => {}
            }
            text
        })
}

proptest! {
    #[test]
    fn every_clean_parse_round_trips(input in text_pieces()) {
        let mut expected = FindQuery::parse(&input);
        prop_assume!(expected.tokens.iter().all(|token| token.message.is_none()));
        expected.tokens.clear();
        let printed = expected.to_text();
        let mut parsed = FindQuery::parse(&printed);
        parsed.tokens.clear();
        prop_assert_eq!(parsed, expected, "printed: {}", printed);
    }
}

#[test]
fn a_regex_prefix_goes_on_a_segment_that_compiles_alone() {
    assert_round_trip(r#""[\"" re:"]""#);
}

#[test]
fn a_tab_after_a_quote_round_trips_exactly() {
    let expected = FindQuery {
        text: "a\"\tb".to_owned(),
        ..FindQuery::default()
    };
    let printed = expected.to_text();
    let mut parsed = FindQuery::parse(&printed);
    parsed.tokens.clear();
    assert_eq!(parsed, expected);
    assert_eq!(printed, "\"a\\\"\"\"\tb\"");
}

#[test]
fn filter_values_with_quotes_round_trip() {
    for input in [
        r#"name:"/a\"\"\" b/""#,
        r#"name:"/a\""" b/""#,
        r#"in:"a\""" b""#,
    ] {
        assert_round_trip(input);
    }
}
