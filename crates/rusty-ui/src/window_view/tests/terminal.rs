use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use rusty_core::AppState;

use super::drive_fs::local;

use super::{fake, open_options, options_for, settle};
use crate::commands::{Keymap, entries};
use crate::window_view::TerminalHooks;

#[gpui_kit::test]
fn terminal_hook_receives_the_active_local_folder(cx: &mut TestAppContext) {
    let fs = fake();
    let folder = local(r"C:\Example Folder");
    let received = Rc::new(RefCell::new(None));
    let captured = received.clone();
    let mut options = options_for(fs, AppState::new(folder.clone()));
    options.terminal = TerminalHooks {
        open: Rc::new(move |folder| {
            captured.replace(Some(folder.clone()));
            Ok(())
        }),
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(crate::window_view::keys::OpenTerminal), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(*received.borrow(), Some(folder));
}

#[gpui_kit::test]
fn terminal_spawn_error_reaches_the_status_line(cx: &mut TestAppContext) {
    let fs = fake();
    let folder = local(r"C:\Example Folder");
    let mut options = options_for(fs, AppState::new(folder));
    options.terminal = TerminalHooks {
        open: Rc::new(|_| Err("terminal failed".to_owned())),
    };
    let (_, view) = open_options(cx, options);
    settle(cx, &view);
    view.update(cx, |view, _| {
        view.open_terminal();
        let pane = view.browser.state().focused_pane();
        assert_eq!(
            view.drives
                .error(pane, view.browser.state().active_tab())
                .unwrap(),
            "terminal failed"
        );
    });
}

#[gpui_kit::test]
fn non_local_folder_refuses_without_calling_the_terminal_hook(cx: &mut TestAppContext) {
    let fs = fake();
    let state = AppState::new(fs.location("home"));
    let mut options = options_for(fs, state);
    options.terminal = TerminalHooks {
        open: Rc::new(|_| panic!("non-local folder reached the launcher")),
    };
    let (_, view) = open_options(cx, options);
    view.update(cx, |view, _| {
        view.open_terminal();
        let pane = view.browser.state().focused_pane();
        assert_eq!(
            view.drives
                .error(pane, view.browser.state().active_tab())
                .unwrap(),
            rusty_l10n::tr("terminal-local-only")
        );
    });
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
