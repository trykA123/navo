//! Keyboard tests of the start screen: the Tab cycle, Enter and Space on the cells, field keys.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, KeyDownEvent, KeyUpEvent, Keystroke, TestAppContext, VisualTestContext,
};
use navo_compare_profile::recent::StartRows;

use crate::start::{Side, StartEvent};
use crate::start_focus::Stop;
use crate::start_tabs::StartMode;
use crate::start_tests::{Screen, TempDir, act, focused, record, row, screen, settle};

#[gpui_kit::test]
fn tab_moves_between_the_fields(cx: &mut TestAppContext) {
    let dir = TempDir::new("tab");
    let s = screen(cx, StartRows::default(), dir.path());
    act(cx, &s, |screen, window, cx| screen.focus_left(window, cx));
    assert_eq!(focused(cx, &s), Some(Side::Left));
    cx.simulate_keystrokes(s.0, "tab");
    assert_eq!(focused(cx, &s), Some(Side::Right));
    cx.simulate_keystrokes(s.0, "shift-tab");
    assert_eq!(focused(cx, &s), Some(Side::Left));
    cx.simulate_keystrokes(s.0, "shift-tab");
    assert_eq!(stop(cx, &s), Some(Stop::File));
}

#[gpui_kit::test]
fn down_and_up_move_between_the_fields_and_the_rows(cx: &mut TestAppContext) {
    let dir = TempDir::new("keys");
    let rows = StartRows {
        recent: vec![row("r1", "a", "b"), row("r2", "c", "d")],
        named: vec![row("n1", "e", "f")],
    };
    let s = screen(cx, rows, dir.path());
    let log = record(cx, &s.1);
    act(cx, &s, |screen, window, cx| screen.focus_left(window, cx));
    cx.simulate_keystrokes(s.0, "down");
    assert_eq!(focused(cx, &s), None);
    cx.simulate_keystrokes(s.0, "enter");
    cx.simulate_keystrokes(s.0, "down down");
    cx.simulate_keystrokes(s.0, "enter");
    cx.simulate_keystrokes(s.0, "down");
    cx.simulate_keystrokes(s.0, "enter");
    cx.simulate_keystrokes(s.0, "up up up");
    assert_eq!(focused(cx, &s), Some(Side::Left));
    let ids: Vec<_> = log
        .borrow()
        .iter()
        .map(|event| match event {
            StartEvent::Open { id } => id.clone(),
            StartEvent::Compare { .. } => "compare".into(),
            StartEvent::Forget { id } => format!("forget {id}"),
        })
        .collect();
    assert_eq!(ids, ["r1", "n1", "n1"]);
}

#[gpui_kit::test]
fn down_with_no_rows_stays_in_the_field(cx: &mut TestAppContext) {
    let dir = TempDir::new("no-rows");
    let s = screen(cx, StartRows::default(), dir.path());
    act(cx, &s, |screen, window, cx| screen.focus_left(window, cx));
    cx.simulate_keystrokes(s.0, "down");
    assert_eq!(focused(cx, &s), Some(Side::Left));
}

/// The stop that holds the focus.
fn stop(cx: &mut TestAppContext, s: &Screen) -> Option<Stop> {
    cx.update_window(s.0, |_, window, cx| s.1.read(cx).focused_stop(window, cx))
        .expect("window")
}

/// Presses and releases one key, as a keyboard does; a click by key fires on the release.
fn press(cx: &mut TestAppContext, s: &Screen, key: &'static str) {
    let keystroke = Keystroke::parse(key).expect("a key");
    let mut vcx = VisualTestContext::from_window(s.0, cx);
    vcx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(KeyUpEvent { keystroke });
}

#[gpui_kit::test]
fn tab_and_shift_tab_cycle_through_every_stop(cx: &mut TestAppContext) {
    let dir = TempDir::new("cycle");
    let s = screen(cx, StartRows::default(), dir.path());
    act(cx, &s, |screen, window, cx| screen.focus_left(window, cx));
    let order = [
        Stop::Right,
        Stop::Compare,
        Stop::Rule,
        Stop::Folder,
        Stop::File,
        Stop::Left,
    ];
    for want in order {
        cx.simulate_keystrokes(s.0, "tab");
        assert_eq!(stop(cx, &s), Some(want));
    }
    cx.simulate_keystrokes(s.0, "shift-tab");
    assert_eq!(stop(cx, &s), Some(Stop::File));
}

#[gpui_kit::test]
fn space_and_enter_activate_the_chip_and_the_tabs(cx: &mut TestAppContext) {
    let dir = TempDir::new("activate");
    let s = screen(cx, StartRows::default(), dir.path());
    let tabs = s.1.read_with(cx, |s, _| s.tabs.clone());
    let choice = |cx: &mut TestAppContext| tabs.read_with(cx, |t, _| t.choice());
    act(cx, &s, |screen, window, cx| {
        screen.focus_side(Side::Right, window, cx)
    });
    cx.simulate_keystrokes(s.0, "tab tab");
    assert_eq!(stop(cx, &s), Some(Stop::Rule));
    press(cx, &s, "space");
    assert!(!choice(cx).respect_gitignore);
    press(cx, &s, "enter");
    assert!(choice(cx).respect_gitignore);
    cx.simulate_keystrokes(s.0, "tab tab");
    press(cx, &s, "enter");
    assert_eq!(stop(cx, &s), Some(Stop::File));
    assert_eq!(choice(cx).mode, StartMode::File);
    let hold = cx
        .update_window(s.0, |_, w, _| {
            w.find("start-right").placeholder().map(str::to_owned)
        })
        .expect("window");
    assert_eq!(hold.as_deref(), Some("File path"));
    cx.simulate_keystrokes(s.0, "shift-tab");
    press(cx, &s, "space");
    assert_eq!(choice(cx).mode, StartMode::Folder);
}

#[gpui_kit::test]
fn enter_on_the_compare_stop_submits(cx: &mut TestAppContext) {
    let dir = TempDir::new("compare-key");
    let s = screen(cx, StartRows::default(), dir.path());
    let log = record(cx, &s.1);
    settle(cx, &s, &dir.text(), &dir.text());
    act(cx, &s, |screen, window, cx| {
        screen.focus_side(Side::Right, window, cx)
    });
    cx.simulate_keystrokes(s.0, "tab");
    assert_eq!(stop(cx, &s), Some(Stop::Compare));
    press(cx, &s, "enter");
    assert_eq!(log.borrow().len(), 1);
}

/// Installs the update global for the prompt card (third launch) and redraws the screen.
fn with_prompt(cx: &mut TestAppContext, s: &Screen) {
    use crate::updates::{Fetch, install_for_test};
    use navo_compare_profile::appearance::{UiPrefs, UpdateCheck};
    use navo_compare_profile::update_state::UpdateState;
    use std::sync::Arc;
    cx.update(|cx| {
        crate::shell_prefs::save_prefs(
            UiPrefs {
                update_check: UpdateCheck::Ask,
                ..UiPrefs::default()
            },
            cx,
        )
        .expect("save");
        let fetch: Fetch = Arc::new(|| Err(sidewise_update::check::CheckError::Unsupported));
        let state = UpdateState {
            launches: 3,
            ..UpdateState::default()
        };
        install_for_test(state, fetch, || 1_000_000, cx);
    });
    cx.update_window(s.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

#[gpui_kit::test]
fn tab_reaches_the_prompt_buttons_and_enter_turns_the_check_on(cx: &mut TestAppContext) {
    let dir = TempDir::new("prompt-keys");
    let s = screen(cx, StartRows::default(), dir.path());
    with_prompt(cx, &s);
    act(cx, &s, |screen, window, cx| screen.focus_left(window, cx));
    cx.simulate_keystrokes(s.0, "tab tab tab");
    assert_eq!(stop(cx, &s), Some(Stop::Rule));
    for want in [Stop::UpdateFirst, Stop::UpdateSecond, Stop::Folder] {
        cx.simulate_keystrokes(s.0, "tab");
        assert_eq!(stop(cx, &s), Some(want));
    }
    for want in [Stop::UpdateSecond, Stop::UpdateFirst, Stop::Rule] {
        cx.simulate_keystrokes(s.0, "shift-tab");
        assert_eq!(stop(cx, &s), Some(want));
    }
    cx.simulate_keystrokes(s.0, "tab");
    press(cx, &s, "enter");
    let choice = cx.update(|cx| crate::shell_prefs::prefs(cx).update_check);
    assert_eq!(choice, navo_compare_profile::appearance::UpdateCheck::On);
}
