use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::menu::{hint, press_key, run_item};
use super::view_parts::note;
use super::view_rig::{Rig, rig, rig_on, shows};
use super::{Setup, redraw};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::SettingsNews;
use crate::window_view::tests::viewer_open;
use crate::window_view::viewer::editor::EditorBody;

const NOTES: &[u8] = b"# Notes\n\nSome **bold** text.\n";
const SPLIT: Field = Field::MarkdownPreviewSplit;

fn split_value(is_shown: bool) -> Change {
    Change {
        field: SPLIT,
        value: Value::Flag(is_shown),
    }
}

fn editing(cx: &mut TestAppContext, rig: Rig) -> Rig {
    rig.press(cx, "ctrl-e");
    rig.settle(cx);
    rig
}

fn markdown(cx: &mut TestAppContext) -> Rig {
    let rig = rig(cx, ("a.md", "b.md"), NOTES);
    editing(cx, rig)
}

fn split_off(cx: &mut TestAppContext) -> Rig {
    let values = Values::default().with(&split_value(false));
    let rig = rig_on(cx, ("a.md", "b.md"), Setup::new(NOTES.to_vec()), values);
    editing(cx, rig)
}

fn has_preview(rig: &Rig, cx: &mut TestAppContext) -> bool {
    shows(cx, rig.window, "editor-preview")
}

fn rows(is_split: bool) -> Vec<(String, bool)> {
    vec![
        ("Line numbers".to_owned(), true),
        ("Preview split".to_owned(), is_split),
        (String::new(), false),
        ("Encoding\u{2026}".to_owned(), false),
        (String::new(), false),
        ("Keep as default".to_owned(), false),
        ("Reset to defaults".to_owned(), false),
    ]
}

fn focused_side(cx: &mut TestAppContext, rig: &Rig) -> Option<&'static str> {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let body = view.read(cx).viewer.as_ref()?.body.as_ref()?;
        let editor = body.as_any().downcast_ref::<EditorBody>()?;
        editor.focused_side(window, cx)
    })
    .expect("window")
}

fn close_and_open_next(cx: &mut TestAppContext, rig: &Rig) {
    press_key(cx, rig.window, "escape");
    for _ in 0..3 {
        rig.press(cx, "escape");
    }
    assert!(!viewer_open(cx, &rig.view));
    rig.press(cx, "down");
    rig.press(cx, "f3");
    rig.settle(cx);
    rig.press(cx, "ctrl-e");
    rig.settle(cx);
}

#[gpui_kit::test]
fn the_markdown_menu_lists_the_split_with_a_check_mark_and_its_key(cx: &mut TestAppContext) {
    let rig = markdown(cx);
    assert!(has_preview(&rig, cx));
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), rows(true));
    assert_eq!(
        hint(cx, &rig.view, "Preview split").as_deref(),
        Some("Ctrl+Shift+V")
    );
}

#[gpui_kit::test]
fn the_split_row_shows_the_setting_when_it_is_off(cx: &mut TestAppContext) {
    let rig = split_off(cx);
    assert!(!has_preview(&rig, cx));
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), rows(false));
}

#[gpui_kit::test]
fn a_text_file_has_no_split_row(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.txt", "b.txt"), NOTES);
    rig.press(cx, "alt-v");
    assert!(!rig.rows(cx).iter().any(|(text, _)| text == "Preview split"));
}

#[gpui_kit::test]
fn the_menu_toggle_changes_only_the_open_file(cx: &mut TestAppContext) {
    let rig = markdown(cx);
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Preview split");
    assert!(rig.is_checked(cx, "Line numbers"));
    assert!(!rig.is_checked(cx, "Preview split"));
    redraw(cx, rig.window);
    assert!(!has_preview(&rig, cx));
    assert_eq!(rig.saved(), []);
    assert_eq!(rig.setting(cx, SPLIT), Value::Flag(true));
    close_and_open_next(cx, &rig);
    assert!(has_preview(&rig, cx), "the next file follows the settings");
}

#[gpui_kit::test]
fn ctrl_shift_v_still_toggles_the_split_and_the_menu_follows(cx: &mut TestAppContext) {
    let rig = markdown(cx);
    rig.press(cx, "ctrl-shift-v");
    assert!(!has_preview(&rig, cx));
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), rows(false));
    press_key(cx, rig.window, "escape");
    rig.press(cx, "ctrl-shift-v");
    assert!(has_preview(&rig, cx));
    assert_eq!(rig.saved(), []);
}

#[gpui_kit::test]
fn ctrl_shift_v_outside_editing_leaves_the_split_alone(cx: &mut TestAppContext) {
    let rig = rig(cx, ("a.md", "b.md"), NOTES);
    rig.press(cx, "ctrl-shift-v");
    rig.press(cx, "ctrl-e");
    rig.settle(cx);
    assert!(has_preview(&rig, cx));
}

#[gpui_kit::test]
fn keep_as_default_saves_the_split_and_the_next_file_follows_it(cx: &mut TestAppContext) {
    let rig = markdown(cx);
    rig.press(cx, "ctrl-shift-v");
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Keep as default");
    rig.settle(cx);
    assert!(rig.menu_items(cx).is_none());
    assert_eq!(rig.saved(), [split_value(false)]);
    assert_eq!(rig.setting(cx, SPLIT), Value::Flag(false));
    assert!(!has_preview(&rig, cx));
    rig.press(cx, "escape");
    rig.press(cx, "down");
    rig.press(cx, "f3");
    rig.settle(cx);
    rig.press(cx, "ctrl-e");
    rig.settle(cx);
    assert!(!has_preview(&rig, cx));
}

#[gpui_kit::test]
fn keep_on_a_broken_settings_file_shows_the_problem_and_keeps_the_split(cx: &mut TestAppContext) {
    let rig = markdown(cx);
    *rig.failure.lock().expect("failure") = Some("navo.toml is invalid at line 3.".to_owned());
    rig.press(cx, "ctrl-shift-v");
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Keep as default");
    rig.settle(cx);
    let shown = note(cx, rig.window).unwrap_or_default();
    assert!(shown.contains("navo.toml is invalid at line 3."), "{shown}");
    assert!(!shown.contains("Kept"), "{shown}");
    assert!(!has_preview(&rig, cx));
    assert_eq!(rig.setting(cx, SPLIT), Value::Flag(true));
    *rig.failure.lock().expect("failure") = None;
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Keep as default");
    rig.settle(cx);
    assert_eq!(
        note(cx, rig.window).as_deref(),
        Some("Kept as the default in navo.toml.")
    );
    assert_eq!(rig.setting(cx, SPLIT), Value::Flag(false));
}

#[gpui_kit::test]
fn reset_to_defaults_brings_the_split_back_to_the_settings(cx: &mut TestAppContext) {
    let rig = markdown(cx);
    rig.press(cx, "ctrl-shift-v");
    assert!(!has_preview(&rig, cx));
    rig.press(cx, "alt-v");
    run_item(cx, rig.window, "Reset to defaults");
    assert!(rig.menu_items(cx).is_none());
    assert!(has_preview(&rig, cx));
    assert_eq!(rig.saved(), []);
}

#[gpui_kit::test]
fn a_settings_change_reaches_the_open_editor_unless_the_split_was_toggled_there(
    cx: &mut TestAppContext,
) {
    let rig = markdown(cx);
    let off = Values::default().with(&split_value(false));
    assert!(rig.news.send(SettingsNews::Applied {
        values: off,
        generation: 1,
    }));
    rig.settle(cx);
    assert!(!has_preview(&rig, cx));
    let on = Values::default();
    assert!(rig.news.send(SettingsNews::Applied {
        values: on,
        generation: 2,
    }));
    rig.settle(cx);
    assert!(has_preview(&rig, cx));
    rig.press(cx, "ctrl-shift-v");
    assert!(rig.news.send(SettingsNews::Applied {
        values: Values::default(),
        generation: 3,
    }));
    rig.settle(cx);
    assert!(!has_preview(&rig, cx), "the viewer's own choice stays");
}

#[gpui_kit::test]
fn a_hidden_split_keeps_its_keys_and_focus_leaves_the_preview(cx: &mut TestAppContext) {
    let rig = markdown(cx);
    rig.press(cx, "f6");
    assert_eq!(focused_side(cx, &rig), Some("preview"));
    rig.press(cx, "ctrl-shift-v");
    assert!(!has_preview(&rig, cx));
    assert_eq!(focused_side(cx, &rig), Some("source"));
    rig.press(cx, "f6");
    assert_eq!(focused_side(cx, &rig), Some("source"));
    rig.press(cx, "ctrl-shift-v");
    assert!(has_preview(&rig, cx));
    rig.press(cx, "f6");
    assert_eq!(focused_side(cx, &rig), Some("preview"));
}

#[gpui_kit::test]
fn ctrl_shift_v_shows_a_split_the_settings_turned_off(cx: &mut TestAppContext) {
    let rig = split_off(cx);
    rig.press(cx, "ctrl-shift-v");
    assert!(has_preview(&rig, cx));
    rig.press(cx, "alt-v");
    assert_eq!(rig.rows(cx), rows(true));
}

#[gpui_kit::test]
fn the_view_key_stays_on_the_bar_while_the_split_is_hidden(cx: &mut TestAppContext) {
    let rig = split_off(cx);
    let keys = super::find::bottom_keys(cx, rig.window);
    assert!(keys.contains(&"Alt+V View".to_owned()), "{keys:?}");
    assert!(
        keys.contains(&"Ctrl+Shift+V Preview".to_owned()),
        "{keys:?}"
    );
}
