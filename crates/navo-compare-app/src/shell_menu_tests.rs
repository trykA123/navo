//! Tests of the tab menu: Close other tabs keeps the active tab, and the menu shows Ctrl+W.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, point, px};

use crate::chrome::sidebar::SidebarEvent;
use crate::menu_host::menu_view;
use crate::shell::{Shell, bind_keys};
use crate::test_window::open;
use navo_compare_profile::recent::StartRows;

/// Opens a shell with `tabs` tabs (keys bound) and renders one frame.
fn shell_with(cx: &mut TestAppContext, tabs: usize) -> (AnyWindowHandle, Entity<Shell>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
    });
    let (window, shell) = open(cx, Shell::new);
    cx.update_window(window, |_, window, cx| {
        shell.update(cx, |shell, cx| {
            for _ in 1..tabs {
                shell.new_tab(window, cx);
            }
        });
        window.render_frame(cx);
    })
    .expect("window");
    (window, shell)
}

/// Makes the sidebar ask for the tab menu of tab `index`, then settles.
fn ask_menu(
    cx: &mut TestAppContext,
    (window, shell): &(AnyWindowHandle, Entity<Shell>),
    index: usize,
) {
    cx.update_window(*window, |_, window, cx| {
        let sidebar = shell.read(cx).sidebar().clone();
        sidebar.update(cx, |_, cx| {
            cx.emit(SidebarEvent::SelectTab(index));
            cx.emit(SidebarEvent::TabMenu(index, point(px(30.), px(30.))));
        });
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

/// Choosing Close other tabs leaves one tab, the one that was active.
#[gpui_kit::test]
fn close_others_keeps_the_active_tab(cx: &mut TestAppContext) {
    let opened = shell_with(cx, 4);
    let (window, shell) = &opened;
    let keep = cx
        .update_window(*window, |_, window, cx| {
            shell.update(cx, |shell, cx| shell.select_tab(2, window, cx));
            shell.read(cx).tab_ids()[2]
        })
        .expect("window");
    cx.update_window(*window, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.close_others(window, cx));
    })
    .expect("window");
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.tab_ids(), vec![keep]);
        assert_eq!(shell.active(), 0);
    });
}

/// The menu lists Close tab with Ctrl+W, and dims Close other tabs when one tab is open.
#[gpui_kit::test]
fn the_tab_menu_shows_ctrl_w(cx: &mut TestAppContext) {
    let opened = shell_with(cx, 1);
    ask_menu(cx, &opened, 0);
    let menu = opened
        .1
        .read_with(cx, |shell, _| menu_view(&shell.menu))
        .expect("open menu");
    menu.read_with(cx, |menu, _| {
        let items = &menu.menu().items;
        assert_eq!(items[0].text, "Close tab");
        assert_eq!(items[0].accelerator.as_deref(), Some("Ctrl+W"));
        assert!(!items[1].enabled);
    });
}

/// A shell with one Recent row in the sidebar, menu keys bound, and the start list focused.
fn shell_with_recent(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Shell>) {
    let opened = shell_with(cx, 1);
    let (window, shell) = &opened;
    cx.update(|cx| {
        crate::menu_host::bind_menu_keys(cx);
    });
    cx.update_window(*window, |_, window, cx| {
        let rows = StartRows {
            recent: vec![crate::start_tests::row("r1", "a", "b")],
            named: vec![],
        };
        let sidebar = shell.read(cx).sidebar().clone();
        sidebar.update(cx, |sidebar, cx| sidebar.set_rows(rows, cx));
        let screen = shell.read(cx).active_screen().expect("start tab").clone();
        let list = screen.read(cx).list_focus.clone();
        window.focus(&list, cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert!(
        list_focused(cx, &opened),
        "the start list holds the focus before the menu"
    );
    opened
}

/// Whether the active start screen's row list holds the focus.
fn list_focused(
    cx: &mut TestAppContext,
    (window, shell): &(AnyWindowHandle, Entity<Shell>),
) -> bool {
    cx.update_window(*window, |_, window, cx| {
        let screen = shell.read(cx).active_screen().expect("start tab").clone();
        screen.read(cx).list_focus.is_focused(window)
    })
    .expect("window")
}

/// Makes the sidebar ask for the menu of Recent entry 0, then settles.
fn ask_recent_menu(cx: &mut TestAppContext, (window, shell): &(AnyWindowHandle, Entity<Shell>)) {
    cx.update_window(*window, |_, window, cx| {
        let sidebar = shell.read(cx).sidebar().clone();
        sidebar.update(cx, |_, cx| {
            cx.emit(SidebarEvent::RecentMenu(0, point(px(30.), px(30.))));
        });
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert!(
        shell.read_with(cx, |shell, _| shell.menu.is_some()),
        "the Recent menu is open"
    );
}

/// A mouse-chosen Copy paths in the Recent menu copies and gives the focus back to the list.
#[gpui_kit::test]
fn recent_copy_paths_by_mouse_refocuses_the_list(cx: &mut TestAppContext) {
    let opened = shell_with_recent(cx);
    ask_recent_menu(cx, &opened);
    cx.update_window(opened.0, |_, w, cx| {
        w.render_frame(cx);
        w.within("context-menu")
            .within(("menu-level", 0usize))
            .click(("menu-row", 1usize), cx);
    })
    .expect("window");
    cx.run_until_parked();
    assert!(opened.1.read_with(cx, |shell, _| shell.menu.is_none()));
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("a\nb"));
    assert!(
        list_focused(cx, &opened),
        "the list had the focus when the menu opened"
    );
}

/// Escape closes the Recent menu and gives the focus back to the list.
#[gpui_kit::test]
fn recent_escape_refocuses_the_list(cx: &mut TestAppContext) {
    let opened = shell_with_recent(cx);
    ask_recent_menu(cx, &opened);
    cx.simulate_keystrokes(opened.0, "escape");
    cx.run_until_parked();
    assert!(opened.1.read_with(cx, |shell, _| shell.menu.is_none()));
    assert!(list_focused(cx, &opened));
}
