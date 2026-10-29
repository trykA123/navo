use super::tab_menu;
use crate::window_view::tests::{fake, open_options, options_for, press};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Modifiers, MouseButton, TestAppContext};
use rusty_core::shell_menu::ItemId;
use rusty_core::{AppState, Command, Direction, TabCommand, apply};

fn enabled(menu: &rusty_core::shell_menu::ShellMenu, id: &'static str) -> bool {
    menu.items
        .iter()
        .find(|item| item.id == ItemId::Action(id))
        .unwrap()
        .enabled
}

#[test]
fn menu_reflects_tab_and_pane_state() {
    let fs = fake();
    let mut state = AppState::new(fs.location("home"));
    let pane = state.focused_pane();
    let menu = tab_menu(&state, pane, 0, &|_| None).unwrap();
    assert!(!enabled(&menu, "tab.close"));
    assert!(!enabled(&menu, "tab.clone"));
    assert!(!enabled(&menu, "tab.reopen"));
    assert!(!enabled(&menu, "tab.close-others"));
    assert!(!enabled(&menu, "tab.close-right"));
    apply(&mut state, &Command::PaneOpenTab { location: None }).unwrap();
    let id = state.pane(pane).unwrap().tabs()[0].id();
    apply(
        &mut state,
        &Command::Tab(TabCommand::Pin {
            tab: Some(id),
            pinned: true,
        }),
    )
    .unwrap();
    let menu = tab_menu(&state, pane, 0, &|_| None).unwrap();
    assert_eq!(menu.items[0].text, rusty_l10n::tr("tab-menu-unpin"));
    assert!(enabled(&menu, "tab.close-right"));
    for _ in 0..3 {
        apply(
            &mut state,
            &Command::LayoutSplit {
                direction: Direction::Right,
            },
        )
        .unwrap();
    }
    let menu = tab_menu(&state, pane, 0, &|_| None).unwrap();
    assert!(enabled(&menu, "tab.clone"));
    let panes = menu
        .items
        .iter()
        .find(|i| i.id == ItemId::Action("tab.panes"))
        .unwrap()
        .children()
        .unwrap();
    assert!(!panes.last().unwrap().enabled);
}

#[test]
fn menu_disables_unavailable_destinations_and_close_others() {
    let fs = fake();
    let mut state = AppState::new(fs.location("home"));
    let pane = state.focused_pane();
    apply(&mut state, &Command::PaneOpenTab { location: None }).unwrap();
    let pinned = state.pane(pane).unwrap().tabs()[0].id();
    apply(
        &mut state,
        &Command::Tab(TabCommand::Pin {
            tab: Some(pinned),
            pinned: true,
        }),
    )
    .unwrap();
    let menu = tab_menu(&state, pane, 1, &|_| None).unwrap();
    assert!(enabled(&menu, "tab.close"));
    assert!(!enabled(&menu, "tab.close-others"));
    let workspaces = menu
        .items
        .iter()
        .find(|item| item.id == ItemId::Action("tab.workspaces"))
        .unwrap()
        .children()
        .unwrap();
    assert!(!workspaces[0].enabled);
    for _ in 1..rusty_core::MAX_WORKSPACES {
        rusty_core::apply_workspace(&mut state, &rusty_core::WorkspaceAction::New { name: None })
            .unwrap();
    }
    let pane = state.focused_pane();
    let menu = tab_menu(&state, pane, 0, &|_| None).unwrap();
    let workspaces = menu
        .items
        .iter()
        .find(|item| item.id == ItemId::Action("tab.workspaces"))
        .unwrap()
        .children()
        .unwrap();
    assert!(!workspaces.last().unwrap().enabled);
}

#[gpui_kit::test]
fn right_click_and_pin_targets_inactive_tab(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = AppState::new(fs.location("home"));
    apply(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(fs.location("home/docs")),
        },
    )
    .unwrap();
    let pane = state.focused_pane();
    let first = state.pane(pane).unwrap().tabs()[0].id();
    let (window, view) = open_options(cx, options_for(fs, state));
    let position = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within(("tabs", pane.get()))
                .find(0usize)
                .bounds()
                .center()
        })
        .unwrap();
    crate::test_pointer::dispatch(
        cx,
        window,
        crate::test_pointer::down(MouseButton::Right, position, Modifiers::default()),
    );
    crate::test_pointer::dispatch(
        cx,
        window,
        crate::test_pointer::up(MouseButton::Right, position, Modifiers::default()),
    );
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.browser.state().active_tab().id(),
            view.browser.state().pane(pane).unwrap().tabs()[1].id()
        );
        assert!(view.tab_menu_view().is_some());
    });
    press(cx, window, &view, "enter");
    view.read_with(cx, |view, _| {
        assert!(view.browser.state().tab(first).unwrap().is_pinned());
        assert!(!view.browser.state().active_tab().is_pinned());
    });
}

#[gpui_kit::test]
fn menu_actions_pin_and_move_the_target_tab(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = AppState::new(fs.location("home"));
    apply(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(fs.location("home/docs")),
        },
    )
    .unwrap();
    let target = state.pane(state.focused_pane()).unwrap().tabs()[0].id();
    rusty_core::apply_workspace(
        &mut state,
        &rusty_core::WorkspaceAction::New {
            name: Some("Work".into()),
        },
    )
    .unwrap();
    rusty_core::apply_workspace(&mut state, &rusty_core::WorkspaceAction::Switch { n: 1 }).unwrap();
    let (window, view) = open_options(cx, options_for(fs.clone(), state));
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.run_tab_menu(target, "tab.pin", window, cx);
            assert!(view.browser.state().tab(target).unwrap().is_pinned());
            view.run_tab_menu(target, "tab.workspace.2", window, cx);
        });
    })
    .unwrap();
    view.read_with(cx, |view, _| {
        assert!(view.browser.state().tab(target).is_none());
        let window = view
            .browser
            .state()
            .window(view.browser.state().focused_window())
            .unwrap();
        assert!(
            window.workspaces()[1]
                .live
                .as_ref()
                .unwrap_or(&window.workspaces()[1].saved)
                .panes()
                .iter()
                .flat_map(|p| p.tabs())
                .any(|t| t.location() == &fs.location("home") && t.is_pinned())
        );
    });
}

#[gpui_kit::test]
fn clone_action_uses_the_inactive_tab(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = AppState::new(fs.location("home"));
    let target = state.active_tab().id();
    apply(
        &mut state,
        &Command::PaneOpenTab {
            location: Some(fs.location("home/docs")),
        },
    )
    .unwrap();
    apply(
        &mut state,
        &Command::LayoutSplit {
            direction: Direction::Right,
        },
    )
    .unwrap();
    let destination = state.focused_pane();
    let (window, view) = open_options(cx, options_for(fs.clone(), state));
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.run_tab_menu(target, "tab.clone", window, cx)
        })
    })
    .unwrap();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.browser
                .state()
                .pane(destination)
                .unwrap()
                .active()
                .location(),
            &fs.location("home")
        )
    });
}
