use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Keystroke, TestAppContext};
use rusty_core::{Command, CursorMove, SpanMode};

use super::{actions, commands, open_list, props, sample_rows};
use crate::file_list::{FileListAction, FileListProps};

fn extend(to: CursorMove) -> Command {
    Command::SelectionExtend {
        to,
        mode: SpanMode::Mark,
    }
}

fn pressed(cx: &mut TestAppContext, props: FileListProps, keys: &[&str]) -> Vec<Command> {
    let (window, harness) = open_list(cx, props);
    cx.update_window(window, |_, window, cx| {
        for key in keys {
            window.press(key, cx);
        }
    })
    .expect("window");
    commands(&harness, cx)
}

#[gpui_kit::test]
fn shift_keys_extend_the_marks(cx: &mut TestAppContext) {
    let keys = [
        "shift-up",
        "shift-down",
        "shift-pageup",
        "shift-pagedown",
        "shift-home",
        "shift-end",
    ];
    let expected = [
        CursorMove::Up,
        CursorMove::Down,
        CursorMove::PageUp,
        CursorMove::PageDown,
        CursorMove::First,
        CursorMove::Last,
    ]
    .map(extend);
    assert_eq!(pressed(cx, props(sample_rows()), &keys), expected);
}

#[gpui_kit::test]
fn ctrl_a_and_ctrl_plus_mark_all(cx: &mut TestAppContext) {
    let sent = pressed(cx, props(sample_rows()), &["ctrl-a", "ctrl-+"]);
    assert_eq!(sent, [Command::SelectionAll, Command::SelectionAll]);
}

#[gpui_kit::test]
fn ctrl_shift_a_and_ctrl_minus_unmark_all(cx: &mut TestAppContext) {
    let sent = pressed(cx, props(sample_rows()), &["ctrl-shift-a", "ctrl--"]);
    assert_eq!(sent, [Command::SelectionClear, Command::SelectionClear]);
}

#[gpui_kit::test]
fn slash_restores_the_marks(cx: &mut TestAppContext) {
    let sent = pressed(cx, props(sample_rows()), &["/"]);
    assert_eq!(sent, [Command::SelectionRestore]);
}

#[gpui_kit::test]
fn slash_restores_and_does_not_type_when_a_filter_is_active(cx: &mut TestAppContext) {
    let filtered = FileListProps {
        filter: Some("pa".into()),
        ..props(sample_rows())
    };
    let (window, harness) = open_list(cx, filtered);
    cx.update_window(window, |_, window, cx| window.press("/", cx))
        .expect("window");
    assert_eq!(commands(&harness, cx), [Command::SelectionRestore]);
    assert_eq!(actions(&harness, cx), Vec::<FileListAction>::new());
}

#[gpui_kit::test]
fn alt_plus_and_alt_minus_mark_and_unmark_the_extension(cx: &mut TestAppContext) {
    let sent = pressed(cx, props(sample_rows()), &["alt-+", "alt--"]);
    assert_eq!(
        sent,
        [
            Command::SelectionExtension { mark: true },
            Command::SelectionExtension { mark: false },
        ]
    );
}

#[test]
fn the_symbol_chords_parse_to_the_key_gpui_reports_on_windows() {
    let cases = [
        ("ctrl-+", "+", true, false),
        ("ctrl--", "-", true, false),
        ("alt-+", "+", false, true),
        ("alt--", "-", false, true),
        ("/", "/", false, false),
    ];
    for (text, key, control, alt) in cases {
        let stroke = Keystroke::parse(text).expect(text);
        assert_eq!(stroke.key, key, "{text}");
        assert_eq!(stroke.modifiers.control, control, "{text}");
        assert_eq!(stroke.modifiers.alt, alt, "{text}");
        assert!(!stroke.modifiers.shift, "{text}");
    }
}
