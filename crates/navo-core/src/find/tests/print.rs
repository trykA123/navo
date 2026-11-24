use crate::find::FindQuery;

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
fn regex_text_keeps_its_prefix_and_quotes_spaces() {
    let query = FindQuery::parse(r#"re:"foo bar""#);
    assert_eq!(query.to_text(), r#"re:"foo bar""#);
}

#[test]
fn in_path_with_spaces_prints_quoted() {
    let query = FindQuery::parse(r#"in:"C:\My files""#);
    assert_eq!(query.to_text(), r#"in:"C:\My files""#);
}
