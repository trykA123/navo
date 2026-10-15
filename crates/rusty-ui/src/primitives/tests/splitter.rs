use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, Entity, IntoElement, ParentElement, Render, Role,
    TestAppContext, Window, div, point, px,
};

use super::open;
use crate::primitives::{Splitter, SplitterState};

struct Split {
    state: Entity<SplitterState>,
}

impl Render for Split {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Splitter::new(
            "split",
            &self.state,
            div().child("left"),
            div().child("right"),
        )
    }
}

fn split(ratio: f32) -> impl FnOnce(&mut Window, &mut gpui_kit::App) -> Split {
    move |_, cx| Split {
        state: cx.new(|_| SplitterState::new(ratio)),
    }
}

fn ratio(view: &Entity<Split>, cx: &mut TestAppContext) -> f32 {
    view.read_with(cx, |view, cx| view.state.read(cx).ratio())
}

fn drag_handle_to(
    x: f32,
    (window, view): &(AnyWindowHandle, Entity<Split>),
    cx: &mut TestAppContext,
) -> f32 {
    cx.update_window(*window, |_, window, cx| {
        window.render_frame(cx);
        let from = window.within("split").find("handle").bounds().center();
        window.drag(from, point(px(x), from.y), cx);
    })
    .expect("window");
    ratio(view, cx)
}

#[test]
fn the_ratio_is_clamped_between_a_tenth_and_nine_tenths() {
    assert_eq!(SplitterState::new(2.0).ratio(), 0.9);
    assert_eq!(SplitterState::new(-1.0).ratio(), 0.1);
    assert_eq!(SplitterState::new(f32::NAN).ratio(), 0.5);
    let mut state = SplitterState::new(0.3);
    assert_eq!(state.ratio(), 0.3);
    state.set_ratio(0.95);
    assert_eq!(state.ratio(), 0.9);
}

#[gpui_kit::test]
fn the_handle_sits_at_the_ratio(cx: &mut TestAppContext) {
    let (window, _) = open(cx, split(0.25));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let handle = window.within("split").find("handle");
        assert_eq!(handle.role(), Some(Role::Splitter));
        let x = handle.bounds().center().x;
        assert!((x - px(200.)).abs() <= px(1.), "handle at {x:?}");
    })
    .expect("window");
}

#[gpui_kit::test]
fn dragging_the_handle_moves_the_ratio(cx: &mut TestAppContext) {
    let opened = open(cx, split(0.5));
    let moved = drag_handle_to(600., &opened, cx);
    assert!((moved - 0.75).abs() < 0.01, "ratio {moved}");
}

#[gpui_kit::test]
fn dragging_past_either_edge_keeps_the_ratio_in_range(cx: &mut TestAppContext) {
    let opened = open(cx, split(0.5));
    assert!((drag_handle_to(0., &opened, cx) - 0.1).abs() < 1e-6);
    assert!((drag_handle_to(799., &opened, cx) - 0.9).abs() < 1e-6);
}
