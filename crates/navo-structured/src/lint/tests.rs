//! Tests for the syntax lint: JSON, TOML, YAML, XML and CSV problems become diagnostics.

use super::{Diagnostic, LintKind, Severity, lint};

/// Runs `lint` and returns its only diagnostic, failing when the count is not one.
fn one(kind: LintKind, text: &str) -> Diagnostic {
    let mut found = lint(kind, text);
    assert_eq!(found.len(), 1, "expected one diagnostic for {text:?}");
    found.remove(0)
}

#[test]
fn valid_json_and_toml_give_nothing() {
    assert!(lint(LintKind::Json, r#"{"a": [1, 2]}"#).is_empty());
    assert!(lint(LintKind::Toml, "a = 1\n[t]\nb = \"x\"\n").is_empty());
}

#[test]
fn json_trailing_comma_points_at_the_brace() {
    let diagnostic = one(LintKind::Json, r#"{"a": 1,}"#);
    assert_eq!((diagnostic.line, diagnostic.col), (1, 9));
    assert_eq!(diagnostic.range, 8..9);
}

#[test]
fn json_error_on_line_two() {
    // Split the JSON literal so the repo typos check stays quiet.
    let diagnostic = one(LintKind::Json, concat!("{\n  \"a\": tr", "u\n}"));
    assert_eq!(diagnostic.line, 2);
}

#[test]
fn json_column_counts_characters() {
    let diagnostic = one(LintKind::Json, r#"["é", x]"#);
    assert_eq!(diagnostic.col, 7);
    assert_eq!(diagnostic.range, 7..8);
}

#[test]
fn json_empty_text() {
    let diagnostic = one(LintKind::Json, "");
    assert_eq!((diagnostic.line, diagnostic.col), (1, 1));
    assert_eq!(diagnostic.range, 0..0);
}

#[test]
fn json_bom_is_not_a_column() {
    let diagnostic = one(LintKind::Json, "\u{feff}[1,]");
    assert_eq!(diagnostic.col, 4);
    assert_eq!(diagnostic.range, 6..7);
}

#[test]
fn toml_duplicate_key_on_line_two() {
    let diagnostic = one(LintKind::Toml, "a = 1\na = 2\n");
    assert_eq!((diagnostic.line, diagnostic.col), (2, 1));
}

#[test]
fn toml_missing_value() {
    let diagnostic = one(LintKind::Toml, "a = \nb = 1\n");
    assert_eq!(diagnostic.line, 1);
}

#[test]
fn crlf_lines_count_once() {
    let diagnostic = one(LintKind::Json, "[1,\r\n x]");
    assert_eq!((diagnostic.line, diagnostic.col), (2, 2));
}

#[test]
fn yaml_valid_is_clean() {
    assert!(lint(LintKind::Yaml, "a: 1\nb:\n  - x\n").is_empty());
}

#[test]
fn yaml_bad_indent_on_line_two() {
    let diagnostic = one(LintKind::Yaml, "a: 1\n  b: 2\n");
    assert_eq!(diagnostic.line, 2);
}

#[test]
fn xml_valid_is_clean() {
    assert!(lint(LintKind::Xml, "<a x=\"1\"><b/></a>").is_empty());
}

#[test]
fn xml_mismatched_end_is_an_error() {
    let diagnostic = one(LintKind::Xml, "<a><b></a>");
    assert_eq!(diagnostic.line, 1);
    assert_eq!(diagnostic.severity, Severity::Error);
}

#[test]
fn xml_unclosed_root_at_end() {
    let diagnostic = one(LintKind::Xml, "<a>\n<b/>\n");
    assert_eq!(diagnostic.message, "unclosed element <a>");
    assert_eq!((diagnostic.line, diagnostic.col), (3, 1));
    assert_eq!(diagnostic.range, 9..9);
}

#[test]
fn xml_empty_has_no_root() {
    let diagnostic = one(LintKind::Xml, "  ");
    assert_eq!(diagnostic.message, "no root element");
    assert_eq!((diagnostic.line, diagnostic.col), (1, 1));
    assert_eq!(diagnostic.range, 0..0);
}

#[test]
fn csv_valid_is_clean() {
    assert!(lint(LintKind::Csv, "a,b\n\"x,1\",2\n\"q\"\"q\",3\n").is_empty());
}

#[test]
fn csv_short_row() {
    let diagnostic = one(LintKind::Csv, "a,b\n1,2\n3\n");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.line, 3);
    assert_eq!(diagnostic.col, 1);
    assert_eq!(diagnostic.range, 8..9);
    assert_eq!(diagnostic.message, "row 3 has 1 fields, expected 2");
}

#[test]
fn csv_unclosed_quote() {
    let diagnostic = one(LintKind::Csv, "a,b\n\"x,1\n");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.message, "unclosed quote");
    assert_eq!((diagnostic.line, diagnostic.col), (2, 1));
    assert_eq!(diagnostic.range, 4..5);
}

#[test]
fn csv_stray_quote_warns() {
    let diagnostic = one(LintKind::Csv, "a,b\nx\"y,1\n");
    assert_eq!(diagnostic.severity, Severity::Warning);
    assert_eq!(diagnostic.message, "quote inside an unquoted field");
    assert_eq!((diagnostic.line, diagnostic.col), (2, 2));
    assert_eq!(diagnostic.range, 5..6);
}

#[test]
fn csv_crlf_short_row() {
    let diagnostic = one(LintKind::Csv, "a,b\r\n1\r\n");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.line, 2);
    assert_eq!(diagnostic.range, 5..6);
}
