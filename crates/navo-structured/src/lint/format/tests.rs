//! Tests for the JSON, TOML and XML formatters and their idempotence.

use serde_json::{Map, Value};

use super::format;
use crate::lint::{LintKind, lint};

/// Returns the formatted text, failing with the lint error when formatting fails.
fn formatted(kind: LintKind, text: &str) -> String {
    let result = format(kind, text);
    assert!(
        result.is_ok(),
        "unexpected error: {:?}",
        result.as_ref().err()
    );
    result.unwrap_or_default()
}

#[test]
fn json_pretty_keeps_order_and_numbers() {
    let source = r#"{"b":1.50,"a":[1,2,{}],"c":"x\"y"}"#;
    let expected =
        "{\n  \"b\": 1.50,\n  \"a\": [\n    1,\n    2,\n    {}\n  ],\n  \"c\": \"x\\\"y\"\n}\n";
    assert_eq!(formatted(LintKind::Json, source), expected);
}

#[test]
fn json_crlf_and_bom_kept() {
    let source = "\u{feff}[1,\r\n2]";
    let expected = "\u{feff}[\r\n  1,\r\n  2\r\n]\r\n";
    assert_eq!(formatted(LintKind::Json, source), expected);
}

#[test]
fn json_invalid_returns_the_lint_error() {
    let source = "{\"a\":1,}";
    let result = format(LintKind::Json, source);
    let expected = lint(LintKind::Json, source).into_iter().next();
    assert_eq!(result.clone().err(), expected);
    assert_eq!(result.err().map(|error| error.line), Some(1));
}

#[test]
fn toml_formats_and_keeps_order() {
    let once = formatted(LintKind::Toml, "b=1\na =  \"x\"\n[t]\nz=2\n");
    let doc = once.parse::<toml_edit::DocumentMut>();
    assert!(doc.is_ok(), "formatted TOML parses");
    let Ok(doc) = doc else { return };
    let keys: Vec<&str> = doc.iter().map(|(key, _)| key).collect();
    assert_eq!(keys, ["b", "a", "t"]);
    let table = doc.get("t").and_then(|item| item.as_table());
    assert!(table.is_some(), "t is a table");
    if let Some(table) = table {
        assert_eq!(table.iter().map(|(key, _)| key).collect::<Vec<_>>(), ["z"]);
    }
    assert_eq!(formatted(LintKind::Toml, &once), once);
}

#[test]
fn xml_indents_nested() {
    let source = r#"<a><b x="1">t</b><c/></a>"#;
    let expected = "<a>\n  <b x=\"1\">t</b>\n  <c/>\n</a>\n";
    assert_eq!(formatted(LintKind::Xml, source), expected);
}

#[test]
fn yaml_is_not_formatted() {
    let message = format(LintKind::Yaml, "a: 1\n")
        .err()
        .map(|error| error.message);
    assert_eq!(
        message,
        Some("formatting is not available for this kind".to_owned())
    );
}

/// Deterministic pseudo-random state so the property test needs no extra crate.
fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 33
}

/// Generates a key of 1 to 3 lowercase ASCII letters.
fn sample_key(state: &mut u64) -> String {
    let len = next(state) % 3 + 1;
    (0..len)
        .map(|_| char::from(b'a' + (next(state) % 26) as u8))
        .collect()
}

/// Builds a random JSON value nested at most `depth` levels deep.
fn sample_value(state: &mut u64, depth: u32) -> Value {
    let choices = if depth == 0 { 3 } else { 5 };
    match next(state) % choices {
        0 => Value::from((next(state) as i64 % 200) - 100),
        1 => Value::Bool(next(state).is_multiple_of(2)),
        2 => Value::Null,
        3 => {
            let len = next(state) % 4;
            Value::Array((0..len).map(|_| sample_value(state, depth - 1)).collect())
        }
        _ => {
            let len = next(state) % 4;
            let mut map = Map::new();
            for _ in 0..len {
                map.insert(sample_key(state), sample_value(state, depth - 1));
            }
            Value::Object(map)
        }
    }
}

#[test]
fn json_format_is_idempotent() {
    let mut state = 0x2545_f491_4f6c_dd1d;
    for _ in 0..200 {
        let value = sample_value(&mut state, 3);
        let text = serde_json::to_string(&value);
        assert!(text.is_ok(), "sample serializes");
        let Ok(text) = text else { continue };
        let once = formatted(LintKind::Json, &text);
        let twice = formatted(LintKind::Json, &once);
        assert_eq!(once, twice);
        let parsed = serde_json::from_str::<Value>(&once);
        assert!(parsed.is_ok(), "formatted text parses");
        if let Ok(parsed) = parsed {
            assert_eq!(parsed, value);
        }
    }
}

#[test]
fn toml_keeps_every_comment() {
    let source = "# top\na = 1 # trailing\n[t]\n# inner\nz=2\n";
    let output = formatted(LintKind::Toml, source);
    assert!(output.contains("# top"), "{output}");
    assert!(output.contains("# trailing"), "{output}");
    assert!(output.contains("# inner"), "{output}");
}

#[test]
fn xml_mixed_content_keeps_spaces() {
    let output = formatted(LintKind::Xml, "<p>Hello <b>big</b> world</p>");
    assert!(output.contains("Hello <b>big</b> world"), "{output}");
}

/// Generates a valid TOML document with comments, keys and at most two tables.
fn sample_toml(state: &mut u64) -> String {
    let mut text = String::from("# top\n");
    let keys = next(state) % 4;
    for index in 0..keys {
        text.push_str(&format!("k{index}={} # c{index}\n", next(state) % 100));
    }
    let tables = next(state) % 3;
    for index in 0..tables {
        text.push_str(&format!(
            "[t{index}]\n# inner {index}\nz{index} = \"v{}\"\n",
            next(state) % 100
        ));
    }
    text
}

/// Generates a valid XML document with comments, CDATA and mixed text children.
fn sample_xml(state: &mut u64) -> String {
    let mut text = String::new();
    if next(state).is_multiple_of(2) {
        text.push_str("<?xml version=\"1.0\"?>\n");
    }
    text.push_str("<root>\n");
    let children = next(state) % 4;
    for index in 0..children {
        match next(state) % 4 {
            0 => text.push_str(&format!("  <!-- c{index} -->\n")),
            1 => text.push_str(&format!("  <item id=\"{index}\">x{index}</item>\n")),
            2 => text.push_str(&format!("  <![CDATA[ raw <{index}> & data ]]>\n")),
            _ => text.push_str(&format!("  <p>Hello <b>big{index}</b> world</p>\n")),
        }
    }
    text.push_str("</root>\n");
    text
}

/// Removes ASCII whitespace so only non-whitespace content is compared.
fn without_whitespace(text: &str) -> String {
    text.chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect()
}

/// Asserts that formatting keeps all non-whitespace content and is idempotent.
fn assert_kept(kind: LintKind, text: &str) {
    let once = formatted(kind, text);
    let twice = formatted(kind, &once);
    assert_eq!(without_whitespace(&once), without_whitespace(text));
    assert_eq!(once, twice);
}

#[test]
fn toml_and_xml_keep_content_and_are_idempotent() {
    let mut state = 0x9e37_79b9_7f4a_7c15;
    for _ in 0..50 {
        assert_kept(LintKind::Toml, &sample_toml(&mut state));
        assert_kept(LintKind::Xml, &sample_xml(&mut state));
    }
}
