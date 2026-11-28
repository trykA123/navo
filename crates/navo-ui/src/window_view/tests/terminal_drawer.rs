use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, point, px};
use navo_core::AppState;
use navo_term::FakeHandle;

use super::drive_fs::local;
use super::{fake, open_options, options_for, settle};
use crate::settings::{Change, Field, Value, Values};
use crate::terminal::{MIN_DRAWER_HEIGHT, TerminalSession};
use crate::window_view::{NavoWindow, SaveReport, SettingsHooks, TerminalHooks};

fn module_values(on: bool) -> Values {
    super::with_fkey_bar(Values::default()).with(&Change {
        field: Field::ModuleTerminal,
        value: Value::Flag(on),
    })
}

fn press_toggle(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
    cx.simulate_keystrokes(window, "ctrl-`");
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
    })
    .unwrap();
}

fn install_fake_backend(
    cx: &mut TestAppContext,
    view: &Entity<NavoWindow>,
) -> Arc<Mutex<Option<FakeHandle>>> {
    let slot: Arc<Mutex<Option<FakeHandle>>> = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&slot);
    view.update(cx, |view, _| {
        view.set_terminal_spawner(Rc::new(move |size, cx| {
            let (session, handle) = TerminalSession::with_fake(size, cx);
            *capture.lock().unwrap() = Some(handle);
            session
        }));
    });
    slot
}

fn is_active_pane_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        let pane = view.read(cx).browser.state().focused_pane();
        view.read(cx)
            .focus
            .get(&pane)
            .is_some_and(|handle| handle.is_focused(window))
    })
    .unwrap()
}

#[gpui_kit::test]
fn terminal_module_off_ctrl_backtick_spawns_no_session_and_shows_notice(cx: &mut TestAppContext) {
    let fs = fake();
    let folder = local(r"C:\Projects\navo");
    let opened_external = Arc::new(Mutex::new(false));
    let capture = Arc::clone(&opened_external);
    let mut options = options_for(fs, AppState::new(folder));
    options.settings.values = module_values(false);
    options.terminal = TerminalHooks {
        open: Arc::new(move |_| {
            *capture.lock().unwrap() = true;
            Ok(())
        }),
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);

    press_toggle(cx, window);

    assert!(
        !view.read_with(cx, |view, _| view.has_terminal_session()),
        "module off must spawn no TerminalSession"
    );
    assert!(
        !view.read_with(cx, |view, _| view.is_terminal_open()),
        "module off must not open the drawer"
    );

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("title-terminal-button").is_none(),
            "title-bar Terminal button stays hidden when module is off"
        );
        let _notice = window
            .try_find("terminal-off-notice")
            .expect("module-off notice must be shown");
        let turn_on = window
            .within("terminal-off-notice")
            .find("terminal-off-turn-on");
        assert_eq!(turn_on.label(), Some("Turn on…"));
        let external = window
            .within("terminal-off-notice")
            .find("terminal-off-external");
        assert_eq!(external.label(), Some("Open in external terminal"));
        window
            .within("terminal-off-notice")
            .click("terminal-off-external", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(
        *opened_external.lock().unwrap(),
        "clicking Open in external terminal runs TerminalHooks::open"
    );
}

#[gpui_kit::test]
fn terminal_module_on_ctrl_backtick_cycles_show_focus_refocus_and_hide(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs, super::two_panes(&fake()));
    options.settings.values = module_values(true);
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    let _fake = install_fake_backend(cx, &view);

    assert!(!view.read_with(cx, |view, _| view.has_terminal_session()));
    assert!(!view.read_with(cx, |view, _| view.is_terminal_open()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("title-terminal-button").is_some(),
            "title-bar Terminal button appears when module is on"
        );
    })
    .unwrap();

    // 1st Ctrl+`: hidden -> show & focus terminal (lazy session spawn)
    press_toggle(cx, window);
    assert!(view.read_with(cx, |view, _| view.has_terminal_session()));
    assert!(view.read_with(cx, |view, _| view.is_terminal_open()));
    assert!(
        cx.update_window(window, |_, window, cx| {
            view.read(cx).is_terminal_focused(window, cx)
        })
        .unwrap(),
        "1st Ctrl+` focuses the terminal"
    );

    // Move focus back to the active file pane while leaving the drawer visible.
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.focus_core_pane(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    assert!(is_active_pane_focused(cx, window, &view));
    assert!(view.read_with(cx, |view, _| view.is_terminal_open()));

    // 2nd Ctrl+`: visible & unfocused -> re-focus terminal
    press_toggle(cx, window);
    assert!(view.read_with(cx, |view, _| view.is_terminal_open()));
    assert!(
        cx.update_window(window, |_, window, cx| {
            view.read(cx).is_terminal_focused(window, cx)
        })
        .unwrap(),
        "2nd Ctrl+` re-focuses the visible terminal"
    );

    // 3rd Ctrl+`: visible & focused -> hide drawer & restore focus to active pane
    press_toggle(cx, window);
    assert!(!view.read_with(cx, |view, _| view.is_terminal_open()));
    assert!(
        is_active_pane_focused(cx, window, &view),
        "3rd Ctrl+` restores focus to active pane"
    );
}

#[gpui_kit::test]
fn terminal_typing_a_and_ctrl_c_reaches_fake_backend(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs, super::two_panes(&fake()));
    options.settings.values = module_values(true);
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    let fake_slot = install_fake_backend(cx, &view);

    press_toggle(cx, window);
    cx.simulate_keystrokes(window, "a ctrl-c");
    cx.run_until_parked();

    let written = fake_slot
        .lock()
        .unwrap()
        .as_ref()
        .expect("fake backend spawned")
        .written();
    assert_eq!(written, b"a\x03");
}

#[gpui_kit::test]
fn terminal_drawer_enforces_minimum_height_and_splitter_persists_setting(cx: &mut TestAppContext) {
    let fs = fake();
    let saved_changes: Arc<Mutex<Vec<Change>>> = Arc::new(Mutex::new(Vec::new()));
    let capture = Arc::clone(&saved_changes);
    let initial = module_values(true);
    let mut options = options_for(fs, super::two_panes(&fake()));
    options.settings = SettingsHooks {
        values: initial.clone(),
        save: Arc::new(move |changes| {
            capture.lock().unwrap().extend_from_slice(changes);
            let next = changes
                .iter()
                .fold(initial.clone(), |acc, change| acc.with(change));
            SaveReport::saved(next)
        }),
        ..SettingsHooks::none()
    };
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    let _fake = install_fake_backend(cx, &view);

    press_toggle(cx, window);

    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let drawer = window.find("terminal-drawer");
        assert!(
            drawer.bounds().size.height >= MIN_DRAWER_HEIGHT,
            "drawer height {:?} below minimum {:?}",
            drawer.bounds().size.height,
            MIN_DRAWER_HEIGHT
        );
        let handle = window.within("terminal-drawer-split").find("handle");
        let from = handle.bounds().center();
        window.drag(from, point(from.x, from.y - px(60.0)), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();

    let persisted = saved_changes
        .lock()
        .unwrap()
        .iter()
        .any(|change| change.field == Field::TerminalDrawerHeight);
    assert!(
        persisted,
        "dragging drawer splitter persists terminal.drawer-height"
    );
}
