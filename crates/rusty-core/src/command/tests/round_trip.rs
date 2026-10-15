use super::*;

fn round_trip(text: &str) -> Result<(Command, String), ParseError> {
    parse(text).map(|c| {
        let printed = c.to_string();
        (c, printed)
    })
}

fn expect(command: Command, canonical: &str) -> Result<(Command, String), ParseError> {
    Ok((command, canonical.to_owned()))
}

#[test]
fn pane_open_tab_with_location_round_trips() {
    let text = r#"pane.open-tab { location: "C:\\Users\\admin" }"#;
    let command = Command::PaneOpenTab {
        location: Some(location(r"C:\Users\admin")),
    };
    assert_eq!(round_trip(text), expect(command, text));
}

#[test]
fn pane_open_tab_without_location_duplicates_the_current_tab() {
    let command = Command::PaneOpenTab { location: None };
    assert_eq!(
        round_trip("pane.open-tab"),
        expect(command, "pane.open-tab")
    );
}

#[test]
fn pane_close_tab_with_index_round_trips() {
    let text = "pane.close-tab { tab: 2 }";
    let command = Command::PaneCloseTab { tab: Some(2) };
    assert_eq!(round_trip(text), expect(command, text));
}

#[test]
fn pane_close_tab_without_index_closes_the_current_tab() {
    let command = Command::PaneCloseTab { tab: None };
    assert_eq!(
        round_trip("pane.close-tab"),
        expect(command, "pane.close-tab")
    );
}

#[test]
fn pane_navigate_round_trips() {
    let text = r#"pane.navigate { location: "/home/admin" }"#;
    let command = Command::PaneNavigate {
        location: location("/home/admin"),
    };
    assert_eq!(round_trip(text), expect(command, text));
}

#[test]
fn pane_back_round_trips() {
    assert_eq!(
        round_trip("pane.back"),
        expect(Command::PaneBack, "pane.back")
    );
}

#[test]
fn pane_forward_round_trips() {
    assert_eq!(
        round_trip("pane.forward"),
        expect(Command::PaneForward, "pane.forward")
    );
}

#[test]
fn pane_cursor_move_keywords_round_trip() {
    let cases = [
        ("up", CursorMove::Up),
        ("down", CursorMove::Down),
        ("page-up", CursorMove::PageUp),
        ("page-down", CursorMove::PageDown),
        ("first", CursorMove::First),
        ("last", CursorMove::Last),
    ];
    let texts: Vec<String> = cases
        .iter()
        .map(|(k, _)| format!("pane.cursor-move {{ to: {k} }}"))
        .collect();
    let actual: Vec<_> = texts.iter().map(|t| round_trip(t)).collect();
    let expected: Vec<_> = cases
        .iter()
        .zip(&texts)
        .map(|((_, to), t)| expect(Command::PaneCursorMove { to: *to }, t))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn pane_cursor_move_to_row_round_trips() {
    let text = "pane.cursor-move { to: 42 }";
    let command = Command::PaneCursorMove {
        to: CursorMove::Row(42),
    };
    assert_eq!(round_trip(text), expect(command, text));
}

#[test]
fn selection_toggle_round_trips() {
    assert_eq!(
        round_trip("selection.toggle"),
        expect(Command::SelectionToggle, "selection.toggle")
    );
}

#[test]
fn selection_toggle_down_round_trips() {
    assert_eq!(
        round_trip("selection.toggle-down"),
        expect(Command::SelectionToggleDown, "selection.toggle-down")
    );
}

#[test]
fn selection_range_round_trips() {
    let text = "selection.range { to: 7 }";
    assert_eq!(
        round_trip(text),
        expect(Command::SelectionRange { to: 7 }, text)
    );
}

#[test]
fn selection_add_pattern_round_trips() {
    let text = r#"selection.add-pattern { pattern: "*.rs" }"#;
    let command = Command::SelectionAddPattern {
        pattern: "*.rs".to_owned(),
    };
    assert_eq!(round_trip(text), expect(command, text));
}

#[test]
fn selection_remove_pattern_round_trips() {
    let text = r#"selection.remove-pattern { pattern: "?.txt" }"#;
    let command = Command::SelectionRemovePattern {
        pattern: "?.txt".to_owned(),
    };
    assert_eq!(round_trip(text), expect(command, text));
}

#[test]
fn selection_invert_round_trips() {
    assert_eq!(
        round_trip("selection.invert"),
        expect(Command::SelectionInvert, "selection.invert")
    );
}

#[test]
fn selection_all_round_trips() {
    assert_eq!(
        round_trip("selection.all"),
        expect(Command::SelectionAll, "selection.all")
    );
}

#[test]
fn selection_clear_round_trips() {
    assert_eq!(
        round_trip("selection.clear"),
        expect(Command::SelectionClear, "selection.clear")
    );
}

#[test]
fn layout_split_round_trips() {
    let text = "layout.split { direction: right }";
    let command = Command::LayoutSplit {
        direction: Direction::Right,
    };
    assert_eq!(round_trip(text), expect(command, text));
}

#[test]
fn layout_close_round_trips() {
    assert_eq!(
        round_trip("layout.close"),
        expect(Command::LayoutClose, "layout.close")
    );
}

#[test]
fn layout_focus_round_trips() {
    let text = "layout.focus { direction: up }";
    let command = Command::LayoutFocus {
        direction: Direction::Up,
    };
    assert_eq!(round_trip(text), expect(command, text));
}

#[test]
fn app_save_session_round_trips() {
    assert_eq!(
        round_trip("app.save-session"),
        expect(Command::AppSaveSession, "app.save-session")
    );
}

#[test]
fn canonical_examples_print_unchanged() {
    let printed: Vec<_> = CANONICAL_EXAMPLES.iter().map(|t| printed(t)).collect();
    let expected: Vec<_> = CANONICAL_EXAMPLES
        .iter()
        .map(|t| Ok((*t).to_owned()))
        .collect();
    assert_eq!(printed, expected);
}
