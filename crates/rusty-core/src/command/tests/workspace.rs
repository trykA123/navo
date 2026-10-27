use super::*;

#[test]
fn workspace_commands_parse_and_print_round_trip() {
    let commands = [
        Command::WorkspaceSwitch { n: 9 },
        Command::WorkspaceNew { name: None },
        Command::WorkspaceNew {
            name: Some("  Project \"📁\"  ".into()),
        },
        Command::WorkspaceRename {
            n: 1,
            name: "New name".into(),
        },
        Command::WorkspaceMove { n: 2, by: -1 },
        Command::WorkspaceMove { n: 1, by: 1 },
        Command::WorkspaceDelete { n: 3 },
        Command::WorkspaceRestore,
        Command::WorkspaceSave,
        Command::WorkspaceDiscard { n: 4 },
    ];
    for command in commands {
        assert_eq!(parse(&command.to_string()), Ok(command));
    }
}

#[test]
fn workspace_numbers_reject_out_of_range_and_wrong_types() {
    for command in ["switch", "rename", "move", "delete", "discard"] {
        for value in ["0", "10", "-1", "18446744073709551616", "\"1\"", "true"] {
            let extra = match command {
                "rename" => ", name: \"Project\"",
                "move" => ", by: 1",
                _ => "",
            };
            let text = format!("workspace.{command} {{ n: {value}{extra} }}");
            assert!(
                matches!(parse(&text).unwrap_err().kind,
                ParseErrorKind::InvalidValue { field, .. } | ParseErrorKind::WrongType { field, .. } if field == "n"),
                "{text}"
            );
        }
    }
}

#[test]
fn workspace_move_rejects_invalid_steps() {
    for value in ["-2", "0", "2", "128", "\"1\"", "true"] {
        let text = format!("workspace.move {{ n: 1, by: {value} }}");
        assert!(matches!(parse(&text).unwrap_err().kind,
            ParseErrorKind::InvalidValue { field, .. } | ParseErrorKind::WrongType { field, .. } if field == "by"));
    }
}

#[test]
fn workspace_commands_validate_required_and_unknown_fields() {
    for (text, field) in [
        ("workspace.switch", "n"),
        ("workspace.rename { n: 1 }", "name"),
        ("workspace.move { n: 1 }", "by"),
        ("workspace.delete", "n"),
        ("workspace.discard", "n"),
    ] {
        assert!(matches!(parse(text).unwrap_err().kind,
            ParseErrorKind::MissingField { field: missing, .. } if missing == field));
    }
    for text in [
        "workspace.new { name: 1 }",
        "workspace.rename { n: 1, name: false }",
    ] {
        assert!(matches!(parse(text).unwrap_err().kind,
            ParseErrorKind::WrongType { field, .. } if field == "name"));
    }
    for text in ["workspace.save { n: 1 }", "workspace.restore { n: 1 }"] {
        assert!(matches!(
            parse(text).unwrap_err().kind,
            ParseErrorKind::UnknownField { .. }
        ));
    }
    assert!(matches!(
        parse("workspace.switch { n: 1, n: 2 }").unwrap_err().kind,
        ParseErrorKind::DuplicateField(_)
    ));
}
