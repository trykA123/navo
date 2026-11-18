use super::*;
use crate::{Delimiter, KeywordGroup, TokenKind};

const FULL: &str = r##"
[[language]]
name = "Demo"
extensions = ["demo", "dm"]
case_ignored = true
line_comments = ["//", "#"]
block_comments = [["/*", "*/"]]
operators = ["+", "=="]
numbers = true

[[language.keywords]]
kind = "keyword"
words = ["if", "else"]
prefix = false

[[language.keywords]]
kind = "variable"
words = ["$"]
prefix = true

[[language.delimiters]]
open = '"'
close = '"'
escape = '\'
multiline = false
kind = "string"

[[language.delimiters]]
open = "<<"
close = ">>"
multiline = true
kind = "delimiter"
"##;

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

#[test]
fn full_language_parses() {
    let expected = Language {
        name: "Demo".to_owned(),
        extensions: owned(&["demo", "dm"]),
        case_ignored: true,
        keywords: vec![
            KeywordGroup {
                kind: TokenKind::Keyword,
                words: owned(&["if", "else"]),
                prefix: false,
            },
            KeywordGroup {
                kind: TokenKind::Variable,
                words: owned(&["$"]),
                prefix: true,
            },
        ],
        line_comments: owned(&["//", "#"]),
        block_comments: vec![("/*".to_owned(), "*/".to_owned())],
        delimiters: vec![
            Delimiter {
                open: "\"".to_owned(),
                close: "\"".to_owned(),
                escape: Some('\\'),
                multiline: false,
                kind: TokenKind::String,
            },
            Delimiter {
                open: "<<".to_owned(),
                close: ">>".to_owned(),
                escape: None,
                multiline: true,
                kind: TokenKind::Delimiter,
            },
        ],
        operators: owned(&["+", "=="]),
        numbers: true,
    };
    assert_eq!(from_toml(FULL), Ok(vec![expected]));
}

#[test]
fn missing_name_is_an_error_that_says_so() {
    let src = FULL.replace("name = \"Demo\"\n", "");
    let err = from_toml(&src).map(|langs| langs.len());
    assert!(
        matches!(&err, Err(e) if e.to_string().contains("name")),
        "{err:?}"
    );
}

#[test]
fn unknown_key_is_an_error_naming_the_key() {
    let src = FULL.replace("numbers = true\n", "numbers = true\nshade = \"red\"\n");
    let err = from_toml(&src).map(|langs| langs.len());
    assert!(
        matches!(&err, Err(RulesError::UnknownKey { key }) if key.ends_with("shade")),
        "{err:?}"
    );
}

/// One TOML language with the given extra lines in its table.
fn with(extra: &str) -> String {
    format!("[[language]]\nname = \"big\"\n{extra}\n")
}

#[test]
fn a_comment_or_string_pattern_over_64_bytes_fails_the_file() {
    let long = "a".repeat(65);
    let ok = "a".repeat(64);
    let toml = |extra: String| from_toml(&with(&extra));
    for extra in [
        format!("line_comments = [\"{long}\"]"),
        format!("block_comments = [[\"{long}\", \"*/\"]]"),
        format!("block_comments = [[\"/*\", \"{long}\"]]"),
        format!("[[language.delimiters]]\nopen = \"{long}\"\nclose = \"x\""),
        format!("[[language.delimiters]]\nopen = \"x\"\nclose = \"{long}\""),
    ] {
        assert!(toml(extra.clone()).is_err(), "{extra}");
    }
    let fits = format!("line_comments = [\"{ok}\"]\nblock_comments = [[\"{ok}\", \"{ok}\"]]");
    assert!(toml(fits).is_ok(), "64 bytes is allowed");
}

#[test]
fn a_language_with_more_than_65535_openers_fails_the_file() {
    let many = |count: usize| {
        let lines: Vec<String> = (0..count).map(|n| format!("\"c{n}\"")).collect();
        from_toml(&with(&format!("line_comments = [{}]", lines.join(", "))))
    };
    assert!(many(65_535).is_ok());
    assert!(many(65_536).is_err());
}
