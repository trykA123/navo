use std::rc::Rc;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};

use super::{open, press};
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
