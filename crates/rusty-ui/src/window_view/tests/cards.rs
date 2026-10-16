use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::PaneId;

use super::{open, press};
use crate::window_view::FerriteWindow;

struct Look {
    pane: PaneId,
    is_focused: bool,
    is_ringed: Option<bool>,
    has_cursor_bar: bool,
}

fn looks(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<FerriteWindow>,
) -> Vec<Look> {
    let (panes, focused) = view.read_with(cx, |view, _| {
        (view.panes(), view.browser.state().focused_pane())
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        panes
            .iter()
            .map(|pane| Look {
                pane: *pane,
                is_focused: *pane == focused,
                is_ringed: window.find(("pane", pane.get())).selected(),
                has_cursor_bar: window
                    .within(("list", pane.get()))
                    .try_find("cursor-bar")
                    .is_some(),
            })
            .collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_accent_ring_follows_the_focused_pane(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    let before = looks(cx, window, &view);
    press(cx, window, &view, "tab");
    let after = looks(cx, window, &view);
    assert_eq!(before.len(), 2);
    for look in before.iter().chain(&after) {
        assert_eq!(look.is_ringed, Some(look.is_focused), "{:?}", look.pane);
    }
    let ringed = |looks: &[Look]| {
        looks
            .iter()
            .find(|l| l.is_ringed == Some(true))
            .map(|l| l.pane)
    };
    assert_ne!(ringed(&before), ringed(&after));
}

#[gpui_kit::test]
fn only_the_active_card_draws_the_cursor_bar(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    for _ in 0..2 {
        let shown = looks(cx, window, &view);
        for look in &shown {
            assert_eq!(look.has_cursor_bar, look.is_focused, "{:?}", look.pane);
        }
        press(cx, window, &view, "tab");
    }
}

#[gpui_kit::test]
fn the_cards_are_named_left_and_right(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    let panes = view.read_with(cx, |view, _| view.panes());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let names: Vec<Option<String>> = panes
            .iter()
            .map(|pane| window.find(("pane", pane.get())).label().map(str::to_owned))
            .collect();
        assert_eq!(
            names,
            [Some("Left pane".to_owned()), Some("Right pane".to_owned())]
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn the_plus_button_opens_a_new_tab_in_its_pane(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    let panes = view.read_with(cx, |view, _| view.panes());
    let tabs = |cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            panes
                .iter()
                .map(|pane| {
                    view.browser
                        .state()
                        .pane(*pane)
                        .map_or(0, |p| p.tabs().len())
                })
                .collect::<Vec<_>>()
        })
    };
    let before = tabs(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within(("pane", panes[1].get())).click("new-tab", cx);
    })
    .expect("window");
    super::settle(cx, &view);
    assert_eq!(tabs(cx), [before[0], before[1] + 1]);
}

#[gpui_kit::test]
fn ctrl_t_opens_a_new_tab_in_the_focused_pane(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    let count = |cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            let state = view.browser.state();
            state
                .pane(state.focused_pane())
                .map_or(0, |p| p.tabs().len())
        })
    };
    let before = count(cx);
    press(cx, window, &view, "ctrl-t");
    assert_eq!(count(cx), before + 1);
}
