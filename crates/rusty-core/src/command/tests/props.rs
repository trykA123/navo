use proptest::prelude::*;

use super::*;

fn tricky_char() -> impl Strategy<Value = char> {
    prop_oneof![
        Just('"'),
        Just('\\'),
        Just('\n'),
        Just('\r'),
        Just('\t'),
        Just('\u{0}'),
        Just('\u{7}'),
        Just('\u{1b}'),
        Just('\u{7f}'),
        Just('\u{85}'),
        Just('{'),
        Just('}'),
        Just(','),
        Just(':'),
        Just(' '),
        Just('é'),
        Just('ß'),
        Just('中'),
        Just('📁'),
        Just('\u{200d}'),
        any::<char>(),
    ]
}

fn any_text(min: usize) -> impl Strategy<Value = String> {
    prop::collection::vec(tricky_char(), min..16).prop_map(|cs| cs.into_iter().collect())
}

fn any_location() -> impl Strategy<Value = Location> {
    any_text(1)
        .prop_map(|s| s.replace('\0', "\u{1}"))
        .prop_map(|s| location(&s))
}

fn any_direction() -> impl Strategy<Value = Direction> {
    prop::sample::select(Direction::ALL.to_vec())
}

fn any_cursor_move() -> impl Strategy<Value = CursorMove> {
    prop_oneof![
        Just(CursorMove::Up),
        Just(CursorMove::Down),
        Just(CursorMove::PageUp),
        Just(CursorMove::PageDown),
        Just(CursorMove::First),
        Just(CursorMove::Last),
        any::<u64>().prop_map(CursorMove::Row),
    ]
}

fn any_command() -> impl Strategy<Value = Command> {
    prop_oneof![
        prop::option::of(any_location()).prop_map(|location| Command::PaneOpenTab { location }),
        prop::option::of(any::<u32>()).prop_map(|tab| Command::PaneCloseTab { tab }),
        any_location().prop_map(|location| Command::PaneNavigate { location }),
        Just(Command::PaneBack),
        Just(Command::PaneForward),
        any_cursor_move().prop_map(|to| Command::PaneCursorMove { to }),
        Just(Command::SelectionToggle),
        Just(Command::SelectionToggleDown),
        any::<u64>().prop_map(|to| Command::SelectionRange { to }),
        any_text(0).prop_map(|pattern| Command::SelectionAddPattern { pattern }),
        any_text(0).prop_map(|pattern| Command::SelectionRemovePattern { pattern }),
        Just(Command::SelectionInvert),
        Just(Command::SelectionAll),
        Just(Command::SelectionClear),
        any_direction().prop_map(|direction| Command::LayoutSplit { direction }),
        Just(Command::LayoutClose),
        any_direction().prop_map(|direction| Command::LayoutFocus { direction }),
        Just(Command::AppSaveSession),
    ]
}

fn any_ws() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(vec![' ', '\t', '\r', '\n']), 0..3)
        .prop_map(|cs| cs.into_iter().collect())
}

fn loosen(canonical: &str, ws: &[String; 6], trailing_comma: bool) -> String {
    let [w0, w1, w2, w3, w4, w5] = ws;
    let body = match canonical.split_once(" { ") {
        None => format!("{canonical}{w1}{{{w2}}}"),
        Some((id, rest)) => {
            let (name, tail) = rest.split_once(": ").unwrap_or((rest, ""));
            let value = tail.strip_suffix(" }").unwrap_or(tail);
            let comma = if trailing_comma { "," } else { "" };
            format!("{id}{w1}{{{w2}{name}{w3}:{w4}{value}{comma}{w2}}}")
        }
    };
    format!("{w0}{body}{w5}")
}

fn token_soup() -> impl Strategy<Value = String> {
    let token = prop::sample::select(vec![
        "pane.back",
        "pane.navigate",
        "selection.range",
        "pane.cursor-move",
        "layout.split",
        "{",
        "}",
        ",",
        ":",
        " ",
        "\n",
        "\"",
        "\\",
        "\\u{",
        "\\u{D800}",
        "\\u{1F4C1}",
        "location",
        "to",
        "tab",
        "direction",
        "-",
        "0",
        "18446744073709551616",
        "up",
        "page-",
        "é",
        "📁",
    ]);
    prop::collection::vec(token, 0..24).prop_map(|ts| ts.concat())
}

proptest! {
    #[test]
    fn any_command_survives_print_and_parse(command in any_command()) {
        prop_assert_eq!(parse(&command.to_string()), Ok(command));
    }

    #[test]
    fn printing_is_canonical(
        command in any_command(),
        ws in [any_ws(), any_ws(), any_ws(), any_ws(), any_ws(), any_ws()],
        trailing_comma in any::<bool>(),
    ) {
        let loose = loosen(&command.to_string(), &ws, trailing_comma);
        let once = printed(&loose).map_err(|e| TestCaseError::fail(format!("{loose:?}: {e}")))?;
        prop_assert_eq!(printed(&once), Ok(once.clone()));
    }

    #[test]
    fn parse_never_panics_on_arbitrary_text(text in any::<String>()) {
        let _ = parse(&text);
    }

    #[test]
    fn parse_never_panics_on_command_like_text(text in token_soup()) {
        let _ = parse(&text);
    }
}
