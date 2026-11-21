//! Tests of the shared menu host: dimmed lines, Enter runs the action on the host, Escape
//! closes and refocuses, and the menu key count.

use gpui_kit::{
    AppContext as _, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, Styled, TestAppContext, Window, div, point, px,
};

use crate::menu_host::{MenuAt, MenuLine, MenuSlot, bind_menu_keys, menu_view, show_menu};
use crate::test_window::open;

gpui_kit::actions!(menu_host_tests, [Ping, Pong]);

/// A small host view: counts `Ping`, holds a menu slot, and draws the open menu.
struct Host {
    focus: FocusHandle,
    slot: MenuSlot,
    pings: usize,
}

impl Render for Host {
    /// Draws the host box with its open menu as a child.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .key_context("Host")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Ping, _, _| this.pings += 1))
            .children(menu_view(&self.slot))
    }
}

impl Focusable for Host {
    /// The host's focus handle.
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Opens a window with a `Host`, shows a menu of `Ping`, `Pong` (disabled), and returns them.
fn host_with_menu(cx: &mut TestAppContext) -> (gpui_kit::AnyWindowHandle, gpui_kit::Entity<Host>) {
    let (window, host) = open(cx, |_, cx| Host {
        focus: cx.focus_handle(),
        slot: None,
        pings: 0,
    });
    cx.update_window(window, |_, window, cx| {
        let focus = host.read(cx).focus.clone();
        window.focus(&focus, cx);
        host.update(cx, |this, cx| {
            let lines = [
                MenuLine::Run {
                    label: "Ping".into(),
                    action: Box::new(Ping),
                    enabled: true,
                },
                MenuLine::Run {
                    label: "Pong".into(),
                    action: Box::new(Pong),
                    enabled: false,
                },
                MenuLine::Break,
            ];
            let at = MenuAt {
                anchor: point(px(40.), px(40.)),
                focus,
                title: "test",
            };
            show_menu(this, (&lines, at, |host| &mut host.slot), window, cx);
        });
    })
    .expect("window");
    cx.run_until_parked();
    (window, host)
}

/// The second line is dimmed because its `MenuLine` is disabled.
#[gpui_kit::test]
fn the_menu_dims_disabled_lines(cx: &mut TestAppContext) {
    let (_, host) = host_with_menu(cx);
    let menu = host
        .read_with(cx, |host, _| menu_view(&host.slot))
        .expect("open menu");
    menu.read_with(cx, |menu, _| {
        assert!(menu.menu().items[0].enabled);
        assert!(!menu.menu().items[1].enabled);
    });
}

/// Enter on the first line runs `Ping` on the host and closes the menu.
#[gpui_kit::test]
fn enter_runs_the_chosen_action_on_the_host(cx: &mut TestAppContext) {
    let (window, host) = host_with_menu(cx);
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    assert_eq!(host.read_with(cx, |host, _| host.pings), 1);
    assert!(host.read_with(cx, |host, _| host.slot.is_none()));
}

/// Escape closes the menu and the host holds the focus again.
#[gpui_kit::test]
fn escape_closes_and_refocuses_the_host(cx: &mut TestAppContext) {
    let (window, host) = host_with_menu(cx);
    assert!(host.read_with(cx, |host, _| host.slot.is_some()));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(host.read_with(cx, |host, _| host.slot.is_none()));
    let focused = cx
        .update_window(window, |_, window, cx| {
            host.read(cx).focus.is_focused(window)
        })
        .expect("window");
    assert!(focused);
    assert_eq!(host.read_with(cx, |host, _| host.pings), 0);
}

/// Twelve keys are bound in each of the two menu contexts.
#[gpui_kit::test]
fn every_menu_key_binds(cx: &mut TestAppContext) {
    let added = cx.update(|cx| {
        gpui_kit::init(cx);
        bind_menu_keys(cx)
    });
    assert_eq!(added, 24);
}
