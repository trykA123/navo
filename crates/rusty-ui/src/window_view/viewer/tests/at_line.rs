use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::{Log, Setup, editor_of, hooks, redraw};
use crate::prelude::SharedString;
use crate::settings::{Field, Values};
use crate::window_view::tests::{fake, home_beside_dest, open_options, options_for, press, settle};
use crate::window_view::viewer::merged_note;
use crate::window_view::{FerriteWindow, KeyLine, SaveReport, SettingsHooks, StartOptions};

const TOML: &[u8] = b"# mine\n[appearance]\nmode = \"dark\"\ndensity = \"compact\"\n";

fn settings_window(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let file = fs.location("home/ferrite.toml");
    let text = String::from_utf8_lossy(TOML).into_owned();
    let options = StartOptions {
        files: hooks(Setup::new(TOML.to_vec()), &Log::default()),
        settings: SettingsHooks {
            file: Some(file),
            line_of: Arc::new(move |key| Some(line_in(&text, key))),
            ..SettingsHooks::none()
        },
        ..options_for(fs.clone(), home_beside_dest(&fs))
    };
    open_options(cx, options)
}

fn line_in(text: &str, key: &str) -> KeyLine {
    let (section, name) = key.rsplit_once('.').unwrap_or_default();
    let found = |wanted: String| {
        text.lines()
            .position(|line| line.starts_with(&wanted))
            .and_then(|at| u64::try_from(at + 1).ok())
    };
    if let Some(line) = found(format!("{name} =")) {
        return KeyLine {
            line,
            is_missing: false,
        };
    }
    let last = u64::try_from(text.lines().count()).unwrap_or(1);
    KeyLine {
        line: found(format!("[{section}]")).unwrap_or(last),
        is_missing: true,
    }
}

fn viewer_note(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find("viewer-message")
            .and_then(|found| found.label().map(str::to_owned))
    })
    .expect("window")
}

fn cursor_line(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<u32> {
    let editor = editor_of(cx, view)?;
    Some(editor.read_with(cx, |state, _| state.cursor_position().line))
}

fn open_at_key(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    press(cx, window, view, "f3");
    for _ in 0..50 {
        cx.run_until_parked();
        settle(cx, view);
        if editor_of(cx, view).is_some() {
            break;
        }
    }
    redraw(cx, window);
}

#[gpui_kit::test]
fn f3_on_a_row_opens_ferrite_toml_at_its_key(cx: &mut TestAppContext) {
    let (window, view) = settings_window(cx);
    press(cx, window, &view, "ctrl-,");
    press(cx, window, &view, "down");
    open_at_key(cx, window, &view);
    let location = view.read_with(cx, |view, _| view.viewer_location().cloned());
    assert!(location.is_some_and(|at| at.as_str().ends_with("ferrite.toml")));
    assert_eq!(cursor_line(cx, &view), Some(3));
}

#[gpui_kit::test]
fn a_key_that_is_not_in_the_file_opens_at_its_section_header_with_a_note(cx: &mut TestAppContext) {
    let (window, view) = settings_window(cx);
    press(cx, window, &view, "ctrl-,");
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    open_at_key(cx, window, &view);
    assert_eq!(cursor_line(cx, &view), Some(1));
    let note = viewer_note(cx, window).unwrap_or_default();
    assert!(note.contains("appearance.light-theme"), "{note}");
    assert!(note.contains("the default applies"), "{note}");
}

#[test]
fn the_missing_key_note_stays_beside_a_note_from_opening_the_file() {
    let start = Some(SharedString::from("mode is missing; the default applies."));
    let both = merged_note(Some("Shown as text."), start.clone());
    assert_eq!(
        both.as_deref(),
        Some("Shown as text. mode is missing; the default applies.")
    );
    assert_eq!(merged_note(None, start.clone()), start);
    assert_eq!(
        merged_note(Some("Shown as text."), None).as_deref(),
        Some("Shown as text.")
    );
}

#[gpui_kit::test]
fn a_key_whose_section_is_not_in_the_file_opens_at_the_last_line(cx: &mut TestAppContext) {
    let (window, view) = settings_window(cx);
    press(cx, window, &view, "ctrl-,");
    press(cx, window, &view, "tab");
    open_at_key(cx, window, &view);
    assert_eq!(cursor_line(cx, &view), Some(3));
}

#[gpui_kit::test]
fn a_key_in_the_file_opens_without_a_note(cx: &mut TestAppContext) {
    let (window, view) = settings_window(cx);
    press(cx, window, &view, "ctrl-,");
    press(cx, window, &view, "down");
    open_at_key(cx, window, &view);
    assert_eq!(viewer_note(cx, window), None);
}

#[gpui_kit::test]
fn closing_the_viewer_returns_to_the_open_sheet(cx: &mut TestAppContext) {
    let (window, view) = settings_window(cx);
    press(cx, window, &view, "ctrl-,");
    open_at_key(cx, window, &view);
    press(cx, window, &view, "escape");
    assert!(view.read_with(cx, |view, _| view.viewer.is_none()));
    assert!(view.read_with(cx, |view, _| view.settings.is_open()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("settings-body").focused(), Some(true));
    })
    .expect("window");
}

#[gpui_kit::test]
fn ctrl_comma_in_the_viewer_leaves_settings_and_focus_alone(cx: &mut TestAppContext) {
    let (window, view) = settings_window(cx);
    press(cx, window, &view, "ctrl-,");
    open_at_key(cx, window, &view);
    let focused = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, cx| window.focused(cx))
            .expect("window")
    };
    let before = focused(cx);
    assert!(before.is_some());
    press(cx, window, &view, "ctrl-,");
    assert!(view.read_with(cx, |view, _| view.settings.is_open()));
    assert!(view.read_with(cx, |view, _| view.viewer.is_some()));
    assert_eq!(focused(cx), before);
}

type Calls = Arc<std::sync::Mutex<Vec<&'static str>>>;

fn waiting_window(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>, Calls) {
    let fs = fake();
    let file = fs.location("home/ferrite.toml");
    let disk = Arc::new(std::sync::Mutex::new(
        "# mine\n[appearance]\nmode = \"dark\"\n".to_owned(),
    ));
    let log = Calls::default();
    let (saving, finding) = (log.clone(), log.clone());
    let written = disk.clone();
    let options = StartOptions {
        files: hooks(Setup::new(TOML.to_vec()), &super::Log::default()),
        settings: SettingsHooks {
            file: Some(file),
            save: Arc::new(move |changes| {
                saving.lock().expect("log").push("save");
                if changes.iter().any(|change| change.field == Field::Density) {
                    written
                        .lock()
                        .expect("disk")
                        .push_str("density = \"compact\"\n");
                }
                let values = changes
                    .iter()
                    .fold(Values::default(), |all, one| all.with(one));
                SaveReport::saved(values)
            }),
            line_of: Arc::new(move |key| {
                finding.lock().expect("log").push("line");
                Some(line_in(&disk.lock().expect("disk"), key))
            }),
            ..SettingsHooks::none()
        },
        ..options_for(fs.clone(), home_beside_dest(&fs))
    };
    let (window, view) = open_options(cx, options);
    (window, view, log)
}

#[gpui_kit::test(iterations = 24)]
fn f3_right_after_a_change_waits_for_its_save(cx: &mut TestAppContext) {
    let (window, view, log) = waiting_window(cx);
    press(cx, window, &view, "ctrl-,");
    press(cx, window, &view, "down");
    cx.update_window(window, |_, window, cx| {
        window.press("left", cx);
        window.press("f3", cx);
    })
    .expect("window");
    for _ in 0..50 {
        cx.run_until_parked();
        settle(cx, &view);
        if editor_of(cx, &view).is_some() {
            break;
        }
    }
    redraw(cx, window);
    assert_eq!(*log.lock().expect("log"), ["save", "line"]);
    assert_eq!(cursor_line(cx, &view), Some(3));
}
