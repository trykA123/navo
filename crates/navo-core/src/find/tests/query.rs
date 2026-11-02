use crate::find::{Attribute, Date, FindQuery, ModifiedFilter, SizeFilter};

#[test]
fn words_and_quoted_phrases_form_text() {
    let query = FindQuery::parse("hello \"two words\" world");
    assert_eq!(query.text, "hello two words world");
    assert!(!query.case);
    assert!(!query.regex);
}

#[test]
fn extension_list_is_normalized() {
    let query = FindQuery::parse("needle ext:c,.H,map");
    assert_eq!(query.text, "needle");
    assert_eq!(query.ext, ["c", "h", "map"]);
}

#[test]
fn sizes_use_binary_units_and_bounds() {
    for (input, expected) in [
        ("size:<50MB", SizeFilter::LessThan(50 * 1024 * 1024)),
        ("size:>1k", SizeFilter::GreaterThan(1024)),
        ("size:1k..2M", SizeFilter::Range(1024, 2 * 1024 * 1024)),
        ("size:>3B", SizeFilter::GreaterThan(3)),
        ("size:<1G", SizeFilter::LessThan(1024 * 1024 * 1024)),
    ] {
        assert_eq!(FindQuery::parse(input).size, Some(expected), "{input}");
    }
}

#[test]
fn modified_filters_retain_relative_and_calendar_values() {
    for (input, expected) in [
        ("mod:<7d", ModifiedFilter::YoungerThanDays(7)),
        ("mod:>7d", ModifiedFilter::OlderThanDays(7)),
        (
            "mod:>2026-10-01",
            ModifiedFilter::After(Date {
                year: 2026,
                month: 10,
                day: 1,
            }),
        ),
        (
            "mod:<2024-02-29",
            ModifiedFilter::Before(Date {
                year: 2024,
                month: 2,
                day: 29,
            }),
        ),
        ("mod:today", ModifiedFilter::Today),
    ] {
        assert_eq!(FindQuery::parse(input).modified, Some(expected), "{input}");
    }
}

#[test]
fn attributes_can_be_required_or_excluded() {
    let query = FindQuery::parse("attr:hidden,readonly,system,archive,-hidden");
    assert_eq!(query.attrs.len(), 5);
    assert_eq!(query.attrs[0].attribute, Attribute::Hidden);
    assert!(!query.attrs[0].excluded);
    assert_eq!(query.attrs[1].attribute, Attribute::Readonly);
    assert_eq!(query.attrs[2].attribute, Attribute::System);
    assert_eq!(query.attrs[3].attribute, Attribute::Archive);
    assert!(query.attrs[4].excluded);
    assert_eq!(FindQuery::parse("-hidden").attrs, [query.attrs[4]]);
}

#[test]
fn scope_paths_preserve_relative_absolute_and_quoted_values() {
    for folder in ["src", "/tmp", "C:\\work", "\\\\host\\share"] {
        assert_eq!(
            FindQuery::parse(&format!("in:{folder}")).within.as_deref(),
            Some(folder)
        );
    }
    assert_eq!(
        FindQuery::parse("in:\"my folder\"").within.as_deref(),
        Some("my folder")
    );
}

#[test]
fn archive_switches_are_explicit() {
    assert!(!FindQuery::parse("").archives);
    assert!(FindQuery::parse("arc:on").archives);
    assert!(!FindQuery::parse("arc:off").archives);
}

#[test]
fn regex_token_preserves_pattern_and_enables_regex() {
    let query = FindQuery::parse(r"re:foo\s+bar");
    assert_eq!(query.text, r"foo\s+bar");
    assert!(query.regex);
    assert!(query.tokens[0].message.is_none());
}

#[test]
fn spans_are_source_byte_offsets() {
    let input = "é ext:c in:\"a b\" size:bad";
    let query = FindQuery::parse(input);
    let source: Vec<_> = query
        .tokens
        .iter()
        .map(|token| &input[token.span.clone()])
        .collect();
    assert_eq!(source, ["é", "ext:c", "in:\"a b\"", "size:bad"]);
    assert_eq!(
        query.tokens[3].message.as_deref(),
        Some("size: needs a number, like <50MB")
    );
}

#[test]
fn invalid_tokens_preserve_remaining_text_and_valid_filters() {
    for token in [
        "ext:",
        "ext:c,,h",
        "size:no",
        "size:2M..1k",
        "size:>18446744073709551615G",
        "mod:<no",
        "mod:>2026-02-29",
        "attr:unknown",
        "in:",
        "arc:yes",
        "re:[",
        "re:",
    ] {
        let query = FindQuery::parse(&format!("before {token} after ext:rs"));
        assert_eq!(query.text, "before after", "{token}");
        assert_eq!(query.ext, ["rs"], "{token}");
        assert!(query.tokens[1].message.is_some(), "{token}");
    }
}

#[test]
fn filters_without_content_are_valid() {
    let query = FindQuery::parse("ext:rs size:<1M attr:-hidden arc:off");
    assert!(query.text.is_empty());
    assert!(query.tokens.iter().all(|token| token.message.is_none()));
    assert!(FindQuery::parse("").tokens.is_empty());
}

#[test]
fn malformed_quotes_are_reported() {
    let query = FindQuery::parse("before in:\"unfinished folder");
    assert_eq!(query.text, "before");
    assert!(query.tokens[1].message.is_some());
}
