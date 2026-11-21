//! Tests of TH6: the Theme picker hosted by the shell, its save and apply, and the rate gate.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, ReadGlobal as _,
    TestAppContext, VisualTestContext,
};
use navo_compare_profile::appearance::PaletteId;

use crate::chrome::topbar_status::Action;
use crate::compare_tab_tests::{TempDir, shell};
use crate::shell::Shell;
use crate::shell_prefs::{install_prefs, prefs};
use crate::theme::SidewiseTheme;
use crate::theme::gate::MIN_GAP;

type Opened = (AnyWindowHandle, Entity<Shell>);

/// Runs a topbar action on the shell, as the router does.
fn act(cx: &mut TestAppContext, (window, view): &Opened, action: Action) {
    cx.update_window(*window, |_, window, cx| {
        view.update(cx, |shell, cx| shell.on_topbar(action, window, cx))
    })
    .expect("window");
    cx.run_until_parked();
    cx.update_window(*window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

/// A shell with a profile in a temp folder, so picks save.
fn rig(cx: &mut TestAppContext, tag: &str) -> (Opened, TempDir) {
    let profile = TempDir::new(tag);
    let opened = shell(cx);
    cx.update(|cx| install_prefs(Some(profile.path()), cx))
        .expect("install");
    (opened, profile)
}

/// The palette now applied to the window.
fn applied(cx: &mut TestAppContext) -> PaletteId {
    cx.update(|cx| SidewiseTheme::global(cx).palette)
}

#[gpui_kit::test]
fn the_theme_action_opens_and_closes_the_picker(cx: &mut TestAppContext) {
    let (opened, _profile) = rig(cx, "th6-toggle");
    act(cx, &opened, Action::Theme);
    assert!(opened.1.read_with(cx, |s, _| s.theme_open()), "opens");
    act(cx, &opened, Action::Theme);
    assert!(!opened.1.read_with(cx, |s, _| s.theme_open()), "closes");
}

#[gpui_kit::test]
fn theme_and_view_menus_close_each_other(cx: &mut TestAppContext) {
    let (opened, _profile) = rig(cx, "th6-exclusive");
    act(cx, &opened, Action::View);
    assert!(opened.1.read_with(cx, |s, _| s.view_open()));
    act(cx, &opened, Action::Theme);
    let state = |cx: &mut TestAppContext| {
        opened
            .1
            .read_with(cx, |s, _| (s.view_open(), s.theme_open()))
    };
    assert_eq!(state(cx), (false, true), "Theme closes View");
    act(cx, &opened, Action::View);
    assert_eq!(state(cx), (true, false), "View closes Theme");
}

#[gpui_kit::test]
fn a_pick_saves_and_applies(cx: &mut TestAppContext) {
    let (opened, profile) = rig(cx, "th6-pick");
    act(cx, &opened, Action::Theme);
    cx.simulate_keystrokes(opened.0, "down enter");
    assert_eq!(cx.update(|cx| prefs(cx)).palette, PaletteId::Graphite);
    assert_eq!(applied(cx), PaletteId::Graphite);
    let text = std::fs::read_to_string(profile.path().join("settings.toml")).expect("saved");
    assert!(text.contains("graphite"), "saved: {text}");
}

#[gpui_kit::test]
fn picks_inside_the_gap_apply_once_then_the_latest(cx: &mut TestAppContext) {
    let (opened, _profile) = rig(cx, "th6-gate");
    act(cx, &opened, Action::Theme);
    cx.simulate_keystrokes(opened.0, "down enter");
    assert_eq!(applied(cx), PaletteId::Graphite);
    cx.simulate_keystrokes(opened.0, "down enter");
    cx.simulate_keystrokes(opened.0, "down enter");
    assert_eq!(applied(cx), PaletteId::Graphite, "held inside the gap");
    assert_eq!(cx.update(|cx| prefs(cx)).palette, PaletteId::Ember, "saved");
    cx.executor().advance_clock(MIN_GAP);
    cx.run_until_parked();
    assert_eq!(applied(cx), PaletteId::Ember, "the trailing apply");
}

#[gpui_kit::test]
fn escape_closes_and_returns_focus(cx: &mut TestAppContext) {
    let (opened, _profile) = rig(cx, "th6-escape");
    let focus = |cx: &mut TestAppContext| {
        cx.update_window(opened.0, |_, window, cx| window.focused(cx))
            .expect("window")
    };
    let before = focus(cx);
    act(cx, &opened, Action::Theme);
    assert_ne!(focus(cx), before, "the picker holds focus");
    cx.simulate_keystrokes(opened.0, "escape");
    assert!(!opened.1.read_with(cx, |s, _| s.theme_open()));
    assert_eq!(focus(cx), before, "focus returns to the tab");
}

#[gpui_kit::test]
fn alt_t_opens_and_closes_the_picker(cx: &mut TestAppContext) {
    let (opened, _profile) = rig(cx, "th8-toggle");
    cx.simulate_keystrokes(opened.0, "alt-t");
    assert!(opened.1.read_with(cx, |s, _| s.theme_open()), "opens");
    cx.simulate_keystrokes(opened.0, "alt-t");
    assert!(!opened.1.read_with(cx, |s, _| s.theme_open()), "closes");
}

#[gpui_kit::test]
fn alt_t_from_a_text_field_opens_the_picker(cx: &mut TestAppContext) {
    let (opened, _profile) = rig(cx, "th8-field");
    cx.update_window(opened.0, |_, window, cx| {
        opened.1.update(cx, |shell, cx| shell.new_tab(window, cx))
    })
    .expect("window");
    cx.run_until_parked();
    let side = cx
        .update_window(opened.0, |_, window, cx| {
            opened
                .1
                .read(cx)
                .active_screen()
                .and_then(|screen| screen.read(cx).focused_side(window, cx))
        })
        .expect("window");
    assert!(side.is_some(), "a start field has focus");
    cx.simulate_keystrokes(opened.0, "alt-t");
    assert!(opened.1.read_with(cx, |s, _| s.theme_open()), "opens");
}

/// A mouse-down outside the open picker closes it and returns focus to the tab, as Escape does.
#[gpui_kit::test]
fn a_click_outside_closes_the_picker(cx: &mut TestAppContext) {
    let (opened, _profile) = rig(cx, "th6b-outside");
    let focus = |cx: &mut TestAppContext| {
        cx.update_window(opened.0, |_, window, cx| window.focused(cx))
            .expect("window")
    };
    let before = focus(cx);
    act(cx, &opened, Action::Theme);
    assert!(opened.1.read_with(cx, |s, _| s.theme_open()), "opens");
    let pane = cx
        .update_window(opened.0, |_, window, _| window.find("main-pane").bounds())
        .expect("window");
    let at = gpui_kit::point(
        pane.left() + gpui_kit::px(8.),
        pane.bottom() - gpui_kit::px(8.),
    );
    VisualTestContext::from_window(opened.0, cx).simulate_mouse_down(
        at,
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(!opened.1.read_with(cx, |s, _| s.theme_open()), "closes");
    assert_eq!(focus(cx), before, "focus returns to the tab");
}
