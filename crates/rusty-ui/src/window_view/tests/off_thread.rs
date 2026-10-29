use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

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
