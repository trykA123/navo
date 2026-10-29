use std::cell::Cell;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, InputEvent as _, MouseMoveEvent, Pixels, Point,
    TestAppContext, VisualTestContext, px, size,
};

use super::{open_options_sized, options_for, two_panes};
use crate::window_view::FerriteWindow;
use crate::window_view::input_trace::InputTrace;

thread_local! {
    static RENDERS: Cell<usize> = const { Cell::new(0) };
}

pub(in crate::window_view) fn note_render() {
    RENDERS.with(|renders| renders.set(renders.get() + 1));
}

fn renders() -> usize {
    RENDERS.with(Cell::get)
}

fn open_two_panes(
    cx: &mut TestAppContext,
    trace: Option<&InputTrace>,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    cx.update(InputTrace::hooks(trace));
    let fs = super::fake();
    let state = two_panes(&fs);
    let opened = open_options_sized(cx, options_for(fs, state), px(1000.));
    VisualTestContext::from_window(opened.0, cx).simulate_resize(size(px(1000.), px(600.)));
    cx.run_until_parked();
    cx.update_window(opened.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
    opened
}

fn center_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    path: [(&'static str, u64); 2],
) -> Point<Pixels> {
    cx.update_window(window, |_, window, _| {
        let [scope, target] = path;
        window.within(scope).find(target).bounds().center()
    })
    .expect("window")
}

fn renders_for_move(cx: &mut TestAppContext, window: AnyWindowHandle, to: Point<Pixels>) -> usize {
    let before = renders();
    cx.update_window(window, |_, window, cx| {
        let moved = MouseMoveEvent {
            position: to,
            pressed_button: None,
            modifiers: Default::default(),
        };
        window.dispatch_event(moved.to_platform_input(), cx);
        window.simulate_next_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
    renders() - before
}

fn assert_rows_cost_nothing(cx: &mut TestAppContext, window: AnyWindowHandle, pane: u64) {
    let first = center_of(cx, window, [("list", pane), ("row", 0)]);
    let second = center_of(cx, window, [("list", pane), ("row", 1)]);
    renders_for_move(cx, window, first);
    let within = Point {
        x: first.x + px(3.),
        ..first
    };
    assert_eq!(renders_for_move(cx, window, within), 0, "same row");
    assert_eq!(renders_for_move(cx, window, second), 0, "next row");
    assert_eq!(renders_for_move(cx, window, first), 0, "previous row");
}

fn first_pane(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> u64 {
    view.read_with(cx, |view, _| view.panes()[0].get())
}

#[gpui_kit::test]
fn moving_over_rows_renders_nothing(cx: &mut TestAppContext) {
    let (window, view) = open_two_panes(cx, None);
    let pane = first_pane(cx, &view);
    assert_rows_cost_nothing(cx, window, pane);
}

#[gpui_kit::test]
fn the_input_trace_does_not_force_a_render_per_mouse_move(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().expect("temp dir");
    let trace = InputTrace::new(root.path().join("input.jsonl")).expect("trace");
    let (window, view) = open_two_panes(cx, Some(&trace));
    let pane = first_pane(cx, &view);
    assert_rows_cost_nothing(cx, window, pane);
}

#[gpui_kit::test]
fn moving_inside_a_pane_header_renders_at_most_once(cx: &mut TestAppContext) {
    let (window, view) = open_two_panes(cx, None);
    let pane = first_pane(cx, &view);
    let tabs = center_of(cx, window, [("pane", pane), ("tabs", pane)]);
    let entering = renders_for_move(cx, window, tabs);
    assert!(
        entering <= 1,
        "entering the header rendered {entering} times"
    );
    let within = Point {
        x: tabs.x + px(3.),
        ..tabs
    };
    assert_eq!(renders_for_move(cx, window, within), 0, "inside the header");
}
