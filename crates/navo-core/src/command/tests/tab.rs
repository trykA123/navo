use super::*;
use crate::{TabCommand, TabId};

#[test]
fn tab_commands_without_a_tab_act_on_the_active_tab() {
    assert_eq!(
        parse("tab.close-others"),
        Ok(Command::Tab(TabCommand::CloseOthers { tab: None }))
    );
    assert_eq!(
        parse(r#"tab.rename { label: "" }"#),
        Ok(Command::Tab(TabCommand::Rename {
            tab: None,
            label: String::new(),
        }))
    );
    let pin = Command::Tab(TabCommand::Pin {
        tab: None,
        pinned: false,
    });
    assert_eq!(pin.to_string(), "tab.pin { pinned: false }");
    assert_eq!(parse(&pin.to_string()), Ok(pin));
}

#[test]
fn tab_commands_round_trip_with_an_explicit_tab() {
    let tab = Some(TabId::new(u64::MAX));
    let commands = [
        TabCommand::Pin { tab, pinned: true },
        TabCommand::Rename {
            tab,
            label: "  \"📁\" \\ ".into(),
        },
        TabCommand::MoveToWorkspace { tab, n: 9 },
        TabCommand::MoveToPane { tab, n: 4 },
        TabCommand::MoveToNewPane { tab },
        TabCommand::Reopen,
    ];
    for command in commands.map(Command::Tab) {
        assert_eq!(parse(&command.to_string()), Ok(command));
    }
}

#[test]
fn tab_command_numbers_reject_out_of_range_values() {
    for text in [
        "tab.move-to-workspace { n: 0 }",
        "tab.move-to-workspace { n: 10 }",
        "tab.move-to-pane { n: 0 }",
        "tab.move-to-pane { n: 5 }",
        "tab.close { tab: -1 }",
        "tab.close { tab: \"3\" }",
    ] {
        assert!(
            matches!(
                parse(text).unwrap_err().kind,
                ParseErrorKind::InvalidValue { .. } | ParseErrorKind::WrongType { .. }
            ),
            "{text}"
        );
    }
}

#[test]
fn tab_commands_require_their_fields() {
    for (text, field) in [
        ("tab.pin", "pinned"),
        ("tab.rename { tab: 1 }", "label"),
        ("tab.move-to-workspace", "n"),
        ("tab.move-to-pane { tab: 2 }", "n"),
    ] {
        assert!(
            matches!(parse(text).unwrap_err().kind,
                ParseErrorKind::MissingField { field: missing, .. } if missing == field),
            "{text}"
        );
    }
    assert!(matches!(
        parse("tab.reopen { tab: 1 }").unwrap_err().kind,
        ParseErrorKind::UnknownField { .. }
    ));
}
