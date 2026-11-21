//! Mouse tests of `Sidebar` rows: a middle click on an open tab closes it,
//! a right click selects it and asks for the tab menu.

use gpui_kit::test::{ClickOptions, TestWindowExt};
use gpui_kit::{AppContext as _, MouseButton, TestAppContext};

use super::super::SidebarEvent;
use super::super::sidebar_tests::open_host_with_outside;

/// A middle click on tab B emits `CloseTab(1)` and nothing else, like its close button.
#[gpui_kit::test]
fn a_middle_click_closes_that_tab(cx: &mut TestAppContext) {
    let rig = open_host_with_outside(cx);
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |sb, cx| {
            sb.set_tabs(vec!["A".into(), "B".into(), "C".into()], 0, cx)
        });
        window.render_frame(cx);
        window.click_with_options(
            ("open-tab", 1usize),
            ClickOptions::new().with_button(MouseButton::Middle),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("middle click");
    assert_eq!(rig.events.borrow().last(), Some(&SidebarEvent::CloseTab(1)));
}

/// A right click on tab B emits `SelectTab(1)` and then `TabMenu(1, _)`.
#[gpui_kit::test]
fn a_right_click_selects_and_asks_for_the_tab_menu(cx: &mut TestAppContext) {
    let rig = open_host_with_outside(cx);
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |sb, cx| {
            sb.set_tabs(vec!["A".into(), "B".into(), "C".into()], 0, cx)
        });
        window.render_frame(cx);
        window.click_with_options(
            ("open-tab", 1usize),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
        window.render_frame(cx);
    })
    .expect("right click");
    let events = rig.events.borrow();
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(events[0], SidebarEvent::SelectTab(1));
    assert!(
        matches!(events[1], SidebarEvent::TabMenu(1, _)),
        "{events:?}"
    );
}
