use std::rc::Rc;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use rusty_core::{Location, PaneId, TabId};

use super::{open, press};
use crate::file_list::{SortBy, SortKey};
use crate::window_view::FerriteWindow;

fn shape(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> [(usize, usize); 2] {
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let focused = state.focused_pane();
        let panes = view.panes();
        let passive = panes.iter().find(|p| **p != focused).copied();
        [Some(focused), passive].map(|pane| {
            pane.and_then(|pane| state.pane(pane))
                .map_or((0, 0), |p| (p.tabs().len(), p.active_index()))
        })
    })
}

fn with_tabs(cx: &mut TestAppContext, extra: usize) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view) = open(cx, Rc::default());
    for _ in 0..extra {
        press(cx, window, &view, "ctrl-t");
    }
    (window, view)
}

fn active(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> usize {
    shape(cx, view)[0].1
}

#[gpui_kit::test]
fn ctrl_tab_walks_forward_and_wraps(cx: &mut TestAppContext) {
    let (window, view) = with_tabs(cx, 2);
    assert_eq!(active(cx, &view), 2);
    press(cx, window, &view, "ctrl-tab");
    assert_eq!(active(cx, &view), 0);
    press(cx, window, &view, "ctrl-tab");
    assert_eq!(active(cx, &view), 1);
}

#[gpui_kit::test]
fn ctrl_shift_tab_walks_back_and_wraps(cx: &mut TestAppContext) {
    let (window, view) = with_tabs(cx, 2);
    press(cx, window, &view, "ctrl-shift-tab");
    assert_eq!(active(cx, &view), 1);
    press(cx, window, &view, "ctrl-shift-tab");
    press(cx, window, &view, "ctrl-shift-tab");
    assert_eq!(active(cx, &view), 2);
}

#[gpui_kit::test]
fn ctrl_page_down_and_up_do_the_same(cx: &mut TestAppContext) {
    let (window, view) = with_tabs(cx, 2);
    press(cx, window, &view, "ctrl-pagedown");
    assert_eq!(active(cx, &view), 0);
    press(cx, window, &view, "ctrl-pageup");
    assert_eq!(active(cx, &view), 2);
}

#[gpui_kit::test]
fn ctrl_number_goes_to_that_tab_and_ctrl_9_to_the_last(cx: &mut TestAppContext) {
    let (window, view) = with_tabs(cx, 3);
    press(cx, window, &view, "ctrl-2");
    assert_eq!(active(cx, &view), 1);
    press(cx, window, &view, "ctrl-1");
    assert_eq!(active(cx, &view), 0);
    press(cx, window, &view, "ctrl-9");
    assert_eq!(active(cx, &view), 3);
}

#[gpui_kit::test]
fn a_number_past_the_tab_count_does_nothing(cx: &mut TestAppContext) {
    let (window, view) = with_tabs(cx, 1);
    let before = shape(cx, &view);
    press(cx, window, &view, "ctrl-5");
    assert_eq!(shape(cx, &view), before);
}

#[gpui_kit::test]
fn ctrl_w_closes_the_active_tab_but_never_the_last(cx: &mut TestAppContext) {
    let (window, view) = with_tabs(cx, 1);
    press(cx, window, &view, "ctrl-w");
    assert_eq!(shape(cx, &view)[0].0, 1);
    press(cx, window, &view, "ctrl-w");
    assert_eq!(shape(cx, &view)[0], (1, 0));
}

#[gpui_kit::test]
fn the_passive_pane_keeps_its_tabs(cx: &mut TestAppContext) {
    let (window, view) = with_tabs(cx, 2);
    let passive = shape(cx, &view)[1];
    press(cx, window, &view, "ctrl-tab");
    press(cx, window, &view, "ctrl-2");
    press(cx, window, &view, "ctrl-w");
    assert_eq!(shape(cx, &view)[1], passive);
}

#[gpui_kit::test]
fn ctrl_shift_zero_centres_the_splitter(cx: &mut TestAppContext) {
    let (window, view) = open(cx, Rc::default());
    view.update(cx, |view, cx| {
        view.split.update(cx, |split, _| split.set_ratio(0.3));
    });
    press(cx, window, &view, "ctrl-shift-0");
    let ratio = view.read_with(cx, |view, cx| view.split.read(cx).ratio());
    assert_eq!(ratio, 0.5);
}

fn prepare_clone(
    cx: &mut TestAppContext,
    view: &Entity<FerriteWindow>,
) -> (PaneId, TabId, Location, SortKey) {
    view.update(cx, |view, _| {
        view.browser.sort_by(SortBy::Size);
        view.browser.sort_by(SortBy::Size);
        view.browser
            .command(&rusty_core::Command::PaneScroll { to: 1 })
            .unwrap();
        let state = view.browser.state();
        let tab = state.active_tab();
        assert_eq!(tab.view().scroll, 1);
        (
            state.focused_pane(),
            tab.id(),
            tab.location().clone(),
            view.browser.sort_key(tab.id()),
        )
    })
}

fn with_clone_source(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = super::fake();
    fs.add_file("home/zeta.txt", 10);
    let (window, view) =
        super::open_options(cx, super::options_for(fs.clone(), super::two_panes(&fs)));
    super::settle(cx, &view);
    press(cx, window, &view, "tab");
    view.update(cx, |view, _| {
        view.browser
            .command(&rusty_core::Command::PaneNavigate {
                location: fs.location("dest"),
            })
            .unwrap();
    });
    super::settle(cx, &view);
    press(cx, window, &view, "tab");
    (window, view)
}

#[gpui_kit::test]
fn ctrl_shift_up_clones_into_the_other_pane_and_keeps_focus(cx: &mut TestAppContext) {
    let (window, view) = with_clone_source(cx);
    for _ in 0..2 {
        let (source, source_tab, location, sort) = prepare_clone(cx, &view);
        let before = shape(cx, &view);
        press(cx, window, &view, "ctrl-shift-up");
        assert_eq!(
            shape(cx, &view),
            [before[0], (before[1].0 + 1, before[1].0)]
        );
        view.read_with(cx, |view, _| {
            let state = view.browser.state();
            assert_eq!(state.focused_pane(), source);
            assert_eq!(state.active_tab().id(), source_tab);
            let other = view
                .panes()
                .into_iter()
                .find(|pane| *pane != source)
                .unwrap();
            let tab = state.pane(other).unwrap().active();
            assert_eq!(tab.location(), &location);
            assert_ne!(tab.id(), source_tab);
            assert_eq!(tab.view().scroll, 0);
            assert_eq!(view.browser.sort_key(tab.id()), sort);
            let names: Vec<_> = tab
                .listing()
                .unwrap()
                .entries()
                .iter()
                .map(|entry| entry.name.as_str())
                .collect();
            assert_eq!(names, ["docs", "zeta.txt", "a.txt"]);
        });
        let focus = view.read_with(cx, |view, _| view.focus[&source].clone());
        assert!(
            cx.update_window(window, |_, window, _| focus.is_focused(window))
                .unwrap()
        );
        press(cx, window, &view, "tab");
    }
}
