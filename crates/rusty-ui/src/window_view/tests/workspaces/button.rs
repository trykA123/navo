use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Modifiers, TestAppContext};

use super::super::fkeys::hold;
use super::super::press;
use super::{ALT, rig, shape, tray};
use crate::window_view::FerriteWindow;

fn label(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) -> Option<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .try_find(id)
            .and_then(|el| el.label().map(str::to_owned))
    })
    .expect("window")
}

fn click_button(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("workspace-button", cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

fn pinned(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> bool {
    view.read_with(cx, |view, _| view.workspace_keys.pinned)
}

fn active_name(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> String {
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let core = state.window(state.focused_window()).expect("core window");
        core.workspaces()[core.active_workspace()].name.clone()
    })
}

#[gpui_kit::test]
fn the_status_bar_shows_the_active_workspace_and_the_count(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    let name = active_name(cx, &view);
    assert_eq!(label(cx, window, "workspace-button-name"), Some(name));
    assert_eq!(
        label(cx, window, "workspace-button-count").as_deref(),
        Some("1/2")
    );
    assert_eq!(label(cx, window, "workspace-button-draft"), None);
}

#[gpui_kit::test]
fn a_new_tab_and_navigation_show_no_draft_in_the_status_bar(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    press(cx, window, &view, "ctrl-t");
    assert_eq!(label(cx, window, "workspace-button-draft"), None);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.apply(
                &rusty_core::Command::PaneNavigate {
                    location: super::super::fake().location("home/docs"),
                },
                window,
                cx,
            );
        });
    })
    .expect("window");
    super::super::settle(cx, &view);
    assert_eq!(label(cx, window, "workspace-button-draft"), None);
}

#[gpui_kit::test]
fn a_click_pins_the_tray_past_the_alt_release(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    click_button(cx, window);
    assert!(tray(cx, &view) && pinned(cx, &view));
    hold(cx, window, ALT);
    hold(cx, window, Modifiers::default());
    assert!(tray(cx, &view));
    assert_eq!(shape(cx, &view), (0, 2));
}

#[gpui_kit::test]
fn alt_number_and_release_still_switch_while_pinned(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    click_button(cx, window);
    hold(cx, window, ALT);
    press(cx, window, &view, "alt-2");
    hold(cx, window, Modifiers::default());
    assert_eq!(shape(cx, &view), (1, 2));
    assert!(tray(cx, &view) && pinned(cx, &view));
    assert_eq!(
        label(cx, window, "workspace-button-count").as_deref(),
        Some("2/2")
    );
}

#[gpui_kit::test]
fn a_second_click_closes_the_pinned_tray(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    click_button(cx, window);
    click_button(cx, window);
    assert!(!tray(cx, &view) && !pinned(cx, &view));
}

#[gpui_kit::test]
fn escape_closes_the_pinned_tray(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    click_button(cx, window);
    press(cx, window, &view, "escape");
    assert!(!tray(cx, &view) && !pinned(cx, &view));
    click_button(cx, window);
    assert!(tray(cx, &view));
}

#[gpui_kit::test]
fn the_app_menu_workspaces_entry_opens_and_pins_the_tray(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    press(cx, window, &view, "f10");
    press(cx, window, &view, "w");
    press(cx, window, &view, "enter");
    assert!(view.read_with(cx, |view, _| view.app_menu.is_none()));
    assert!(tray(cx, &view) && pinned(cx, &view));
    hold(cx, window, ALT);
    hold(cx, window, Modifiers::default());
    assert!(tray(cx, &view));
}

#[gpui_kit::test]
fn a_card_foot_button_works_while_pinned_without_alt(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    let first = active_name(cx, &view);
    click_button(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("workspace-card", 1usize))
            .click(("workspace-move-right", 1usize), cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
    let second = view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let core = state.window(state.focused_window()).expect("core window");
        core.workspaces()[1].name.clone()
    });
    assert_eq!(second, first);
    assert!(tray(cx, &view) && pinned(cx, &view));
}

#[gpui_kit::test]
fn one_workspace_button_opens_the_tray_and_new_creates_the_second(cx: &mut TestAppContext) {
    let (window, view) = super::super::open(cx, std::rc::Rc::default());
    hold(cx, window, Modifiers::default());
    cx.update(|cx| cx.set_reduce_motion(true));
    assert_eq!(shape(cx, &view), (0, 1));
    click_button(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("workspace-tray").visible());
        assert!(window.find(("workspace-card", 1usize)).visible());
        assert!(window.find("workspace-new").visible());
        window.click("workspace-new-switch", cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(shape(cx, &view), (1, 2));
}

#[gpui_kit::test]
fn one_workspace_button_opens_the_tray_from_the_path_field(cx: &mut TestAppContext) {
    let (window, view) = super::super::open(cx, std::rc::Rc::default());
    hold(cx, window, Modifiers::default());
    cx.update(|cx| cx.set_reduce_motion(true));
    press(cx, window, &view, "alt-d");
    assert!(view.read_with(cx, |view, _| view.path_edit.is_some()));
    click_button(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("workspace-tray").visible());
        assert!(window.find(("workspace-card", 1usize)).visible());
        assert!(window.find("workspace-new").visible());
    })
    .expect("window");
    assert!(tray(cx, &view) && pinned(cx, &view));
}

#[gpui_kit::test]
fn one_workspace_button_opens_the_tray_while_no_pane_has_focus(cx: &mut TestAppContext) {
    let (window, view) = super::super::open(cx, std::rc::Rc::default());
    hold(cx, window, Modifiers::default());
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(window, |_, window, cx| window.blur(cx))
        .expect("window");
    click_button(cx, window);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("workspace-tray").visible());
        assert!(window.find("workspace-new").visible());
    })
    .expect("window");
    assert!(tray(cx, &view) && pinned(cx, &view));
}
