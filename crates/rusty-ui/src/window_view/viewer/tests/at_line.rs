use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::{Log, Setup, editor_of, hooks, redraw};
use crate::window_view::tests::{fake, home_beside_dest, open_options, options_for, press, settle};
use crate::window_view::{FerriteWindow, SettingsHooks, StartOptions};

const TOML: &[u8] = b"# mine\n[appearance]\nmode = \"dark\"\ndensity = \"compact\"\n";

fn settings_window(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let file = fs.location("home/ferrite.toml");
    let text = String::from_utf8_lossy(TOML).into_owned();
    let options = StartOptions {
        files: hooks(Setup::new(TOML.to_vec()), &Log::default()),
        settings: SettingsHooks {
            file: Some(file),
            line_of: Arc::new(move |key| {
                let line = line_in(&text, key)?;
                Some(line)
            }),
            ..SettingsHooks::none()
        },
        ..options_for(fs.clone(), home_beside_dest(&fs))
    };
    open_options(cx, options)
}

fn line_in(text: &str, key: &str) -> Option<u64> {
    let name = key.rsplit('.').next()?;
    let at = text
        .lines()
        .position(|line| line.starts_with(&format!("{name} =")))?;
    u64::try_from(at + 1).ok()
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
fn a_key_that_is_not_in_the_file_opens_at_the_top(cx: &mut TestAppContext) {
    let (window, view) = settings_window(cx);
    press(cx, window, &view, "ctrl-,");
    press(cx, window, &view, "tab");
    open_at_key(cx, window, &view);
    assert_eq!(cursor_line(cx, &view), Some(0));
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
