mod clipboard;
mod undo;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, InputEvent as _, KeyDownEvent, KeyUpEvent, Keystroke,
    TestAppContext,
};

use super::terminal::open_terminal;
use super::{fake, home_beside_dest, open_options, options_for, press, settle};
use crate::settings::{Change, Field, Values};
use crate::window_view::{EditorHooks, FerriteWindow, SettingsHooks, TerminalHooks};

const SLOW: Duration = Duration::from_millis(300);
const ONE_FRAME: Duration = Duration::from_millis(100);

type Calls = Arc<Mutex<Vec<(String, String)>>>;

fn editor_with(
    cx: &mut TestAppContext,
    open: impl Fn(&str, &rusty_core::Location) -> Result<(), String> + Send + Sync + 'static,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let values = Values::default().with(&Change::text(Field::EditorPath, "vim.exe".to_owned()));
    let options = crate::window_view::StartOptions {
        editor: EditorHooks {
            open: Arc::new(open),
            ..EditorHooks::none()
        },
        settings: SettingsHooks {
            values,
            ..SettingsHooks::none()
        },
        ..options_for(fs, state)
    };
    open_options(cx, options)
}

fn press_f4(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    press(cx, window, view, "down");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press("f4", cx);
    })
    .unwrap();
    settle(cx, view);
}

#[gpui_kit::test]
fn f4_returns_at_once_and_starts_the_editor_once_after_the_worker_finishes(
    cx: &mut TestAppContext,
) {
    let calls: Calls = Arc::default();
    let log = calls.clone();
    let (window, view) = editor_with(cx, move |program, file| {
        std::thread::sleep(SLOW);
        log.lock()
            .unwrap()
            .push((program.to_owned(), file.as_str().to_owned()));
        Ok(())
    });
    press(cx, window, &view, "down");
    let started = Instant::now();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.press("f4", cx);
    })
    .unwrap();
    assert!(started.elapsed() < ONE_FRAME);
    assert!(calls.lock().unwrap().is_empty());
    cx.run_until_parked();
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "vim.exe");
    assert!(calls[0].1.ends_with("a.txt"));
}

#[gpui_kit::test]
fn f4_failure_asks_for_another_editor_with_the_same_error(cx: &mut TestAppContext) {
    let (window, view) = editor_with(cx, |program, _| Err(format!("Could not start {program}")));
    press_f4(cx, window, &view);
    cx.run_until_parked();
    let error = view.read_with(cx, |view, _| {
        view.prompt
            .as_ref()
            .and_then(|prompt| prompt.error.clone())
            .map(|error| error.to_string())
    });
    assert_eq!(error.as_deref(), Some("Could not start vim.exe"));
}

#[gpui_kit::test]
fn two_f4_presses_start_the_editor_in_order(cx: &mut TestAppContext) {
    let calls: Calls = Arc::default();
    let log = calls.clone();
    let (window, view) = editor_with(cx, move |program, file| {
        log.lock()
            .unwrap()
            .push((program.to_owned(), file.as_str().to_owned()));
        Ok(())
    });
    press_f4(cx, window, &view);
    press_f4(cx, window, &view);
    cx.run_until_parked();
    let files: Vec<String> = calls.lock().unwrap().iter().map(|c| c.1.clone()).collect();
    assert_eq!(files.len(), 2);
    assert!(files[0].ends_with("a.txt") || files[0].ends_with("docs"));
}

#[gpui_kit::test]
fn the_terminal_returns_at_once_and_starts_after_the_worker_finishes(cx: &mut TestAppContext) {
    let fs = fake();
    let folder = super::drive_fs::local(r"C:\Example Folder");
    let started = Arc::new(Mutex::new(0_usize));
    let count = started.clone();
    let mut options = options_for(fs, rusty_core::AppState::new(folder));
    options.terminal = TerminalHooks {
        open: Arc::new(move |_| {
            std::thread::sleep(SLOW);
            *count.lock().unwrap() += 1;
            Ok(())
        }),
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    let before = Instant::now();
    open_terminal(cx, window);
    assert!(before.elapsed() < ONE_FRAME);
    assert_eq!(*started.lock().unwrap(), 0);
    cx.run_until_parked();
    assert_eq!(*started.lock().unwrap(), 1);
}

fn key(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let keystroke = Keystroke::parse(key).unwrap();
        let down = KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        };
        let up = KeyUpEvent { keystroke };
        let before = Instant::now();
        window.dispatch_event(down.to_platform_input(), cx);
        window.dispatch_event(up.to_platform_input(), cx);
        assert!(before.elapsed() < ONE_FRAME);
    })
    .unwrap();
}

#[gpui_kit::test]
fn editor_discovery_returns_at_once_and_fills_the_editor_prompt(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs.clone(), home_beside_dest(&fs));
    options.editor.suggestion = Arc::new(|| {
        std::thread::sleep(SLOW);
        Some("discovered.exe".to_owned())
    });
    let resolved = Arc::new(Mutex::new(Vec::new()));
    let log = resolved.clone();
    options.editor.resolve = Arc::new(move |program| {
        log.lock().unwrap().push(program.to_owned());
        Err("missing".to_owned())
    });
    let (window, view) = open_options(cx, options);
    key(cx, window, "f4");
    assert!(view.read_with(cx, |view, _| view.prompt.is_none()));
    settle(cx, &view);
    press(cx, window, &view, "enter");
    assert_eq!(*resolved.lock().unwrap(), ["discovered.exe"]);
}

#[gpui_kit::test]
fn editor_discovery_preserves_a_newer_folder_prompt_and_input(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs.clone(), home_beside_dest(&fs));
    options.editor.suggestion = Arc::new(|| Some("discovered.exe".to_owned()));
    let (window, view) = open_options(cx, options);
    key(cx, window, "f4");
    key(cx, window, "f7");
    let prompt = view.read_with(cx, |view, _| view.prompt.as_ref().unwrap().id);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.input("kept-folder", cx);
    })
    .unwrap();
    settle(cx, &view);
    assert_eq!(
        view.read_with(cx, |view, _| view.prompt.as_ref().unwrap().id),
        prompt
    );
    press(cx, window, &view, "enter");
    assert!(rusty_fs::Provider::metadata(fs.as_ref(), &fs.location("home/kept-folder")).is_ok());
}
