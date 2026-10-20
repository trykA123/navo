use super::*;

#[test]
fn selection_extend_round_trips_every_target_and_mode() {
    let targets = [
        ("up", CursorMove::Up),
        ("down", CursorMove::Down),
        ("page-up", CursorMove::PageUp),
        ("page-down", CursorMove::PageDown),
        ("first", CursorMove::First),
        ("last", CursorMove::Last),
        ("42", CursorMove::Row(42)),
        ("18446744073709551615", CursorMove::Row(u64::MAX)),
    ];
    for (target, to) in targets {
        for mode in [SpanMode::Mark, SpanMode::Unmark, SpanMode::Toggle] {
            let word = mode.keyword();
            let text = format!("selection.extend {{ to: {target}, mode: {word} }}");
            let command = Command::SelectionExtend { to, mode };
            assert_eq!(parse(&text), Ok(command.clone()));
            assert_eq!(command.to_string(), text);
        }
    }
}

#[test]
fn selection_extension_round_trips_both_modes() {
    for mark in [false, true] {
        let text = format!("selection.extension {{ mark: {mark} }}");
        let command = Command::SelectionExtension { mark };
        assert_eq!(parse(&text), Ok(command.clone()));
        assert_eq!(command.to_string(), text);
    }
}

#[test]
fn selection_restore_round_trips() {
    assert_eq!(parse("selection.restore"), Ok(Command::SelectionRestore));
    assert_eq!(Command::SelectionRestore.to_string(), "selection.restore");
}

#[test]
fn selection_commands_reject_missing_fields() {
    let cases = [
        ("selection.extend", CommandId::SelectionExtend, "to"),
        (
            "selection.extend { to: up }",
            CommandId::SelectionExtend,
            "mode",
        ),
        ("selection.extension", CommandId::SelectionExtension, "mark"),
    ];
    for (text, command, field) in cases {
        assert_eq!(
            parse(text).unwrap_err().kind,
            ParseErrorKind::MissingField { command, field }
        );
    }
}

#[test]
fn selection_commands_reject_invalid_arguments() {
    let cases = [
        "selection.extend { to: sideways, mode: mark }",
        "selection.extend { to: -1, mode: mark }",
        "selection.extend { to: 18446744073709551616, mode: mark }",
        "selection.extend { to: \"up\", mode: mark }",
        "selection.extend { to: down, mode: flip }",
        "selection.extend { to: down, mode: 1 }",
        "selection.extend { to: down, mode: \"mark\" }",
        "selection.extend { to: down, mode: mark, mode: toggle }",
        "selection.extend { to: down, unmark: false }",
        "selection.extension { mark: yes }",
        "selection.extension { mark: \"true\" }",
        "selection.extension { mark: 1 }",
        "selection.restore { mark: true }",
    ];
    for text in cases {
        assert!(parse(text).is_err(), "accepted {text}");
    }
}
