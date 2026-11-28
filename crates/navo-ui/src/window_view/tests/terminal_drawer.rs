use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ClipboardItem, Entity, MouseButton, TestAppContext,
    VisualTestContext, point, px,
};
use navo_core::AppState;
use navo_term::{FakeHandle, Size};

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

#[gpui_kit::test]
fn terminal_ctrl_shift_v_pastes_and_f_keys_reach_backend_without_global_hijack(
    cx: &mut TestAppContext,
) {
    let fs = fake();
    let mut options = options_for(fs, super::two_panes(&fake()));
    options.settings.values = module_values(true);
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    let fake_slot = install_fake_backend(cx, &view);

    press_toggle(cx, window);

    // Enable bracketed paste mode on the terminal grid.
    fake_slot
        .lock()
        .unwrap()
        .as_ref()
        .expect("fake backend spawned")
        .emit(b"\x1b[?2004h");
    cx.run_until_parked();

    cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string("hello".to_owned())));
    cx.simulate_keystrokes(window, "ctrl-shift-v");
    cx.run_until_parked();

    let clip_open = cx.update(|cx| crate::clip_palette::window::current(cx).is_some());
    assert!(
        !clip_open,
        "Ctrl+Shift+V in focused terminal must not open Clipboard History"
    );
    let written_after_paste = fake_slot
        .lock()
        .unwrap()
        .as_ref()
        .expect("fake backend spawned")
        .written();
    assert_eq!(
        written_after_paste, b"\x1b[200~hello\x1b[201~",
        "Ctrl+Shift+V must paste clipboard text with bracketed paste framing"
    );

    cx.simulate_keystrokes(window, "f1 f10");
    cx.run_until_parked();

    assert!(
        !view.read_with(cx, |view, _| view.settings.is_open()),
        "F1 in focused terminal must not open Navo Help"
    );
    let written_after_fkeys = fake_slot
        .lock()
        .unwrap()
        .as_ref()
        .expect("fake backend spawned")
        .written();
    assert_eq!(
        written_after_fkeys, b"\x1b[200~hello\x1b[201~\x1bOP\x1b[21~",
        "F1 and F10 in focused terminal must send VT function-key bytes"
    );
}

#[gpui_kit::test]
fn terminal_initial_open_syncs_measured_viewport_size_after_debounce(cx: &mut TestAppContext) {
    let mut handle_slot = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        crate::tokens::apply(crate::tokens::Mode::Light, cx);
        let session = cx.new(|cx| {
            let (session, handle) = TerminalSession::with_fake(Size { cols: 80, rows: 24 }, cx);
            handle_slot = Some(handle);
            session
        });
        cx.open_window(Default::default(), |_, cx| {
            cx.new(|cx| crate::terminal::TerminalDrawer::new(session, |_, _| {}, cx))
        })
        .expect("open drawer window")
    });
    let handle = handle_slot.expect("fake handle");

    // Advance past RESIZE_DEBOUNCE (50 ms) with zero extra renders or user interaction.
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();

    let sizes = handle.sizes();
    let initial = sizes
        .last()
        .copied()
        .expect("initial drawer open must send measured viewport resize after debounce");
    assert_ne!(
        initial,
        Size { cols: 80, rows: 24 },
        "measured viewport resize must reflect real drawer geometry, not stay at 80x24"
    );

    // Resize the window once (single final resize frame) and advance past debounce.
    VisualTestContext::from_window(window.into(), cx)
        .simulate_resize(gpui_kit::size(px(1200.0), px(400.0)));
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();

    let after_resize = handle.sizes();
    let resized = after_resize.last().copied().expect("resized size");
    assert_ne!(
        resized, initial,
        "final resize frame must sync updated (cols, rows) without an extra unrelated render"
    );
}

#[gpui_kit::test]
fn terminal_splitter_drag_coalesces_drawer_height_saves_until_commit(cx: &mut TestAppContext) {
    let fs = fake();
    let save_calls: Arc<Mutex<Vec<Vec<Change>>>> = Arc::new(Mutex::new(Vec::new()));
    let capture = Arc::clone(&save_calls);
    let initial = module_values(true);
    let mut options = options_for(fs, super::two_panes(&fake()));
    options.settings = SettingsHooks {
        values: initial.clone(),
        save: Arc::new(move |changes| {
            capture.lock().unwrap().push(changes.to_vec());
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

    let from = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within("terminal-drawer-split")
                .find("handle")
                .bounds()
                .center()
        })
        .unwrap();

    let mut visual = VisualTestContext::from_window(window, cx);
    visual.simulate_mouse_down(from, MouseButton::Left, Default::default());
    visual.simulate_mouse_move(
        point(from.x, from.y - px(20.0)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_move(
        point(from.x, from.y - px(40.0)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_move(
        point(from.x, from.y - px(60.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();

    assert!(
        save_calls.lock().unwrap().is_empty(),
        "dragging splitter must not persist terminal.drawer-height mid-drag (got {} saves)",
        save_calls.lock().unwrap().len()
    );

    VisualTestContext::from_window(window, cx).simulate_mouse_up(
        point(from.x, from.y - px(60.0)),
        MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();

    let calls = save_calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        1,
        "releasing splitter drag must persist terminal.drawer-height exactly once"
    );
    assert!(
        calls[0]
            .iter()
            .any(|change| change.field == Field::TerminalDrawerHeight),
        "single commit save must contain Field::TerminalDrawerHeight"
    );
}
