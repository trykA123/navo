use super::*;
use ParseErrorKind::*;

fn invalid_value(text: &str) -> Option<(usize, String)> {
    match parse(text) {
        Err(ParseError {
            offset,
            kind: InvalidValue { field, .. },
        }) => Some((offset, field)),
        _ => None,
    }
}

#[test]
fn empty_text_is_unexpected_end_at_zero() {
    assert_eq!(parse(""), error(0, UnexpectedEnd));
}

#[test]
fn whitespace_only_is_unexpected_end_at_its_length() {
    assert_eq!(parse(" \n"), error(2, UnexpectedEnd));
}

#[test]
fn id_without_second_segment_is_unexpected_end() {
    assert_eq!(parse("pane."), error(5, UnexpectedEnd));
}

#[test]
fn unclosed_brace_is_unexpected_end() {
    assert_eq!(parse("pane.back {"), error(11, UnexpectedEnd));
}

#[test]
fn missing_value_is_unexpected_end() {
    assert_eq!(parse("pane.navigate { location:"), error(25, UnexpectedEnd));
}

#[test]
fn uppercase_id_is_unexpected_char() {
    assert_eq!(parse("Pane.back"), error(0, UnexpectedChar('P')));
}

#[test]
fn text_after_the_id_is_unexpected_char() {
    assert_eq!(parse("pane.back x"), error(10, UnexpectedChar('x')));
}

#[test]
fn text_after_the_closing_brace_is_unexpected_char() {
    assert_eq!(parse("pane.back {} ;"), error(13, UnexpectedChar(';')));
}

#[test]
fn missing_colon_is_unexpected_char() {
    assert_eq!(
        parse("selection.range { to 3 }"),
        error(21, UnexpectedChar('3'))
    );
}

#[test]
fn missing_comma_between_fields_is_unexpected_char() {
    assert_eq!(
        parse("selection.range { to: 3 to: 4 }"),
        error(24, UnexpectedChar('t'))
    );
}

#[test]
fn double_comma_is_unexpected_char() {
    assert_eq!(
        parse("selection.range { to: 3,, }"),
        error(24, UnexpectedChar(','))
    );
}

#[test]
fn unknown_id_is_unknown_command_at_its_start() {
    assert_eq!(
        parse("  pane.fly {}"),
        error(2, UnknownCommand("pane.fly".to_owned()))
    );
}

#[test]
fn field_on_a_command_without_fields_is_unknown_field() {
    assert_eq!(
        parse("pane.back { to: 1 }"),
        error(
            12,
            UnknownField {
                command: CommandId::PaneBack,
                field: "to".to_owned()
            }
        )
    );
}

#[test]
fn unknown_field_name_is_unknown_field() {
    assert_eq!(
        parse("selection.range { row: 1 }"),
        error(
            18,
            UnknownField {
                command: CommandId::SelectionRange,
                field: "row".to_owned()
            }
        )
    );
}

#[test]
fn repeated_field_is_duplicate_field_at_the_second_name() {
    assert_eq!(
        parse("selection.range { to: 1, to: 2 }"),
        error(25, DuplicateField("to".to_owned()))
    );
}

#[test]
fn missing_required_field_in_braces_points_at_the_closing_brace() {
    assert_eq!(
        parse("pane.navigate { }"),
        error(
            16,
            MissingField {
                command: CommandId::PaneNavigate,
                field: "location"
            }
        )
    );
}

#[test]
fn missing_required_field_without_braces_points_at_the_end() {
    assert_eq!(
        parse("layout.split"),
        error(
            12,
            MissingField {
                command: CommandId::LayoutSplit,
                field: "direction"
            }
        )
    );
}

#[test]
fn missing_required_field_without_braces_ignores_trailing_whitespace() {
    assert_eq!(
        parse("selection.range \n"),
        error(
            17,
            MissingField {
                command: CommandId::SelectionRange,
                field: "to"
            }
        )
    );
}

#[test]
fn integer_for_a_location_is_wrong_type() {
    assert_eq!(
        parse("pane.navigate { location: 5 }"),
        error(
            26,
            WrongType {
                field: "location".to_owned(),
                expected: "string"
            }
        )
    );
}

#[test]
fn string_for_an_integer_field_is_wrong_type() {
    assert_eq!(
        parse(r#"selection.range { to: "5" }"#),
        error(
            22,
            WrongType {
                field: "to".to_owned(),
                expected: "integer"
            }
        )
    );
}

#[test]
fn integer_for_a_direction_is_wrong_type() {
    assert_eq!(
        parse("layout.focus { direction: 1 }"),
        error(
            26,
            WrongType {
                field: "direction".to_owned(),
                expected: "keyword"
            }
        )
    );
}

#[test]
fn string_for_a_cursor_target_is_wrong_type() {
    assert_eq!(
        parse(r#"pane.cursor-move { to: "up" }"#),
        error(
            23,
            WrongType {
                field: "to".to_owned(),
                expected: "keyword or integer"
            }
        )
    );
}

#[test]
fn empty_location_is_an_invalid_value() {
    assert_eq!(
        invalid_value(r#"pane.navigate { location: "" }"#),
        Some((26, "location".to_owned()))
    );
}

#[test]
fn location_with_nul_is_an_invalid_value() {
    assert_eq!(
        invalid_value(r#"pane.open-tab { location: "a\u{0}b" }"#),
        Some((26, "location".to_owned()))
    );
}

#[test]
fn unknown_direction_keyword_is_an_invalid_value() {
    assert_eq!(
        invalid_value("layout.split { direction: north }"),
        Some((26, "direction".to_owned()))
    );
}

#[test]
fn unknown_cursor_keyword_is_an_invalid_value() {
    assert_eq!(
        invalid_value("pane.cursor-move { to: sideways }"),
        Some((23, "to".to_owned()))
    );
}

#[test]
fn unknown_escape_is_bad_escape_at_the_backslash() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "ab\q" }"#),
        error(36, BadEscape)
    );
}

#[test]
fn surrogate_escape_is_bad_escape() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "\u{D800}" }"#),
        error(34, BadEscape)
    );
}

#[test]
fn escape_above_the_largest_scalar_is_bad_escape() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "\u{110000}" }"#),
        error(34, BadEscape)
    );
}

#[test]
fn escape_with_seven_digits_is_bad_escape() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "\u{0000041}" }"#),
        error(34, BadEscape)
    );
}

#[test]
fn escape_without_digits_is_bad_escape() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "\u{}" }"#),
        error(34, BadEscape)
    );
}

#[test]
fn escape_without_braces_is_bad_escape() {
    assert_eq!(
        parse(r#"selection.add-pattern { pattern: "\u0041" }"#),
        error(34, BadEscape)
    );
}

#[test]
fn unclosed_string_is_unterminated_string_at_the_opening_quote() {
    assert_eq!(
        parse(r#"pane.navigate { location: "C:\\Users"#),
        error(26, UnterminatedString)
    );
}
