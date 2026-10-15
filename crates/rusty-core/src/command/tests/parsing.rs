use super::*;

fn pattern(text: &str) -> Command {
    Command::SelectionAddPattern {
        pattern: text.to_owned(),
    }
}

fn invalid_value(text: &str) -> Option<(usize, String)> {
    match parse(text) {
        Err(ParseError {
            offset,
            kind: ParseErrorKind::InvalidValue { field, .. },
        }) => Some((offset, field)),
        _ => None,
    }
}

#[test]
fn whitespace_and_newlines_are_ignored_between_tokens() {
    let text = " \n\tpane.cursor-move\r\n{\n  to :\tfirst\n}\n ";
    assert_eq!(
        parse(text),
        Ok(Command::PaneCursorMove {
            to: CursorMove::First
        })
    );
}

#[test]
fn trailing_comma_is_allowed() {
    assert_eq!(
        parse("selection.range { to: 3, }"),
        Ok(Command::SelectionRange { to: 3 })
    );
}

#[test]
fn trailing_comma_after_whitespace_is_allowed() {
    assert_eq!(
        parse("selection.range { to: 3 \n, \n}"),
        Ok(Command::SelectionRange { to: 3 })
    );
}

#[test]
fn empty_braces_parse_like_no_braces() {
    assert_eq!(parse("pane.back {}"), Ok(Command::PaneBack));
}

#[test]
fn empty_braces_with_whitespace_parse_like_no_braces() {
    assert_eq!(parse("pane.back { \n }"), Ok(Command::PaneBack));
}

#[test]
fn empty_braces_leave_optional_fields_absent() {
    assert_eq!(
        parse("pane.open-tab {}"),
        Ok(Command::PaneOpenTab { location: None })
    );
}

#[test]
fn empty_braces_print_as_the_id_alone() {
    assert_eq!(printed("pane.back {}"), Ok("pane.back".to_owned()));
}

#[test]
fn messy_text_prints_in_canonical_form() {
    assert_eq!(
        printed("  layout.split{direction:left,}  "),
        Ok("layout.split { direction: left }".to_owned())
    );
}

#[test]
fn unicode_escape_with_six_digits_decodes_an_emoji() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "\u{1F4C1} x" }"#),
        Ok(pattern("\u{1F4C1} x"))
    );
}

#[test]
fn unicode_escape_with_one_digit_decodes() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "\u{9}" }"#),
        Ok(pattern("\t"))
    );
}

#[test]
fn unicode_escape_accepts_the_largest_scalar_value() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "\u{10FFFF}" }"#),
        Ok(pattern("\u{10FFFF}"))
    );
}

#[test]
fn simple_escapes_decode() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "a\"b\\c\nd\re\tf" }"#),
        Ok(pattern("a\"b\\c\nd\re\tf"))
    );
}

#[test]
fn raw_newline_inside_a_string_is_kept() {
    assert_eq!(
        parse("selection.add-pattern { pattern: \"a\nb\" }"),
        Ok(pattern("a\nb"))
    );
}

#[test]
fn location_with_quotes_and_backslashes_parses() {
    let text = r#"pane.navigate { location: "C:\\Users\\admin\\\"x\"" }"#;
    assert_eq!(
        parse(text),
        Ok(Command::PaneNavigate {
            location: location(r#"C:\Users\admin\"x""#)
        })
    );
}

#[test]
fn location_with_quotes_and_backslashes_prints_back_escaped() {
    let text = r#"pane.navigate { location: "C:\\Users\\admin\\\"x\"" }"#;
    assert_eq!(printed(text), Ok(text.to_owned()));
}

#[test]
fn simple_escapes_print_as_backslash_sequences() {
    let text = "selection.add-pattern { pattern: \"\\u{9}\\u{a}\\u{d}\" }";
    assert_eq!(
        printed(text),
        Ok(r#"selection.add-pattern { pattern: "\t\n\r" }"#.to_owned())
    );
}

#[test]
fn other_control_characters_print_as_lowercase_unpadded_hex() {
    let text = "selection.add-pattern { pattern: \"\\u{7}\\u{1B}\\u{0}\" }";
    assert_eq!(
        printed(text),
        Ok(r#"selection.add-pattern { pattern: "\u{7}\u{1b}\u{0}" }"#.to_owned())
    );
}

#[test]
fn non_ascii_prints_as_is() {
    let text = r#"selection.add-pattern { pattern: "ünï \u{1F4C1}" }"#;
    assert_eq!(
        printed(text),
        Ok("selection.add-pattern { pattern: \"ünï 📁\" }".to_owned())
    );
}

#[test]
fn integer_with_leading_zeros_prints_without_them() {
    assert_eq!(
        printed("selection.range { to: 007 }"),
        Ok("selection.range { to: 7 }".to_owned())
    );
}

#[test]
fn largest_u64_fits_a_range_target() {
    assert_eq!(
        parse("selection.range { to: 18446744073709551615 }"),
        Ok(Command::SelectionRange { to: u64::MAX })
    );
}

#[test]
fn largest_u32_fits_a_tab_index() {
    assert_eq!(
        parse("pane.close-tab { tab: 4294967295 }"),
        Ok(Command::PaneCloseTab {
            tab: Some(u32::MAX)
        })
    );
}

#[test]
fn negative_integer_in_an_unsigned_field_is_an_invalid_value() {
    assert_eq!(
        invalid_value("selection.range { to: -1 }"),
        Some((22, "to".to_owned()))
    );
}

#[test]
fn negative_row_is_an_invalid_value() {
    assert_eq!(
        invalid_value("pane.cursor-move { to: -3 }"),
        Some((23, "to".to_owned()))
    );
}

#[test]
fn u64_overflow_is_an_invalid_value() {
    assert_eq!(
        invalid_value("selection.range { to: 18446744073709551616 }"),
        Some((22, "to".to_owned()))
    );
}

#[test]
fn u32_overflow_in_a_tab_index_is_an_invalid_value() {
    assert_eq!(
        invalid_value("pane.close-tab { tab: 4294967296 }"),
        Some((22, "tab".to_owned()))
    );
}

#[test]
fn from_str_matches_parse() {
    let text = "layout.focus { direction: down }";
    assert_eq!(text.parse::<Command>(), parse(text));
}
