use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_core::AppState;

use super::drive_fs::local;

use super::{fake, open_options, options_for, settle};
use crate::commands::{Keymap, entries};
use crate::window_view::{FerriteWindow, TerminalHooks};

pub(super) fn open_terminal(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(crate::window_view::keys::OpenTerminal), cx);
    })
    .unwrap();
}

fn pane_error(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<String> {
    view.read_with(cx, |view, _| {
        let pane = view.browser.state().focused_pane();
        view.drives
            .error(pane, view.browser.state().active_tab())
            .map(|error| error.to_string())
    })
}

#[gpui_kit::test]
fn terminal_hook_receives_the_active_local_folder(cx: &mut TestAppContext) {
    let fs = fake();
    let folder = local(r"C:\Example Folder");
    let received = Arc::new(Mutex::new(None));
    let captured = received.clone();
    let mut options = options_for(fs, AppState::new(folder.clone()));
    options.terminal = TerminalHooks {
        open: Arc::new(move |folder| {
            *captured.lock().unwrap() = Some(folder.clone());
            Ok(())
        }),
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    open_terminal(cx, window);
    cx.run_until_parked();
    assert_eq!(*received.lock().unwrap(), Some(folder));
}

#[gpui_kit::test]
fn terminal_spawn_error_reaches_the_status_line(cx: &mut TestAppContext) {
    let fs = fake();
    let folder = local(r"C:\Example Folder");
    let mut options = options_for(fs, AppState::new(folder));
    options.terminal = TerminalHooks {
        open: Arc::new(|_| Err("terminal failed".to_owned())),
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    open_terminal(cx, window);
    cx.run_until_parked();
    assert_eq!(pane_error(cx, &view).as_deref(), Some("terminal failed"));
}

#[gpui_kit::test]
fn non_local_folder_refuses_without_calling_the_terminal_hook(cx: &mut TestAppContext) {
    let fs = fake();
    let state = AppState::new(fs.location("home"));
    let mut options = options_for(fs, state);
    options.terminal = TerminalHooks {
        open: Arc::new(|_| panic!("non-local folder reached the launcher")),
    };
    let (window, view) = open_options(cx, options);
    open_terminal(cx, window);
    cx.run_until_parked();
    assert_eq!(
        pane_error(cx, &view).as_deref(),
        Some(rusty_l10n::tr("terminal-local-only").as_str())
    );
}

#[test]
fn terminal_command_is_in_the_palette_without_a_default_key() {
    let entry = entries()
        .into_iter()
        .find(|entry| entry.spec.name == "files.open_terminal")
        .unwrap();
    assert!(entry.enabled && entry.in_palette);
    assert_eq!(entry.spec.action, Some("ferrite_window::OpenTerminal"));
    assert_eq!(Keymap::builtin().keys_of(entry.spec.name).count(), 0);
}
