use std::cell::Cell;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, InputEvent as _, Keystroke, MouseMoveEvent, Pixels,
    Point, TestAppContext, VisualTestContext, px, size,
};

use super::offline::{enter_slow, stalled, wait};
use super::pane_popups::{NARROW, Rig};
use super::{open_options_sized, options_for, settle, two_panes};
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
    frame(cx, opened.0);
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
    })
    .expect("window");
    frame(cx, window);
    renders() - before
}

fn frame(cx: &mut TestAppContext, window: AnyWindowHandle) {
    for _ in 0..6 {
        let ran = cx
            .update_window(window, |_, window, cx| window.simulate_next_frame(cx))
            .expect("window");
        cx.run_until_parked();
        if ran == 0 {
            return;
        }
    }
    panic!("the window kept asking for frames");
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

#[derive(Clone, Default)]
struct ChildRenders {
    title: usize,
    panes: std::collections::HashMap<rusty_core::PaneId, usize>,
}

thread_local! {
    static CHILD_RENDERS: std::cell::RefCell<ChildRenders> = Default::default();
}

pub(in crate::window_view) fn note_child_render(part: super::super::cached_views::Part) {
    use super::super::cached_views::Part;
    CHILD_RENDERS.with_borrow_mut(|counts| match part {
        Part::Title => counts.title += 1,
        Part::Pane(pane) => *counts.panes.entry(pane).or_default() += 1,
    });
}

fn child_renders() -> ChildRenders {
    CHILD_RENDERS.with_borrow(Clone::clone)
}

#[gpui_kit::test]
fn hovering_one_pane_keeps_other_composition_cached(cx: &mut TestAppContext) {
    let (window, view) = open_two_panes(cx, None);
    let panes = view.read_with(cx, |view, _| view.panes());
    let tabs = cx
        .update_window(window, |_, window, _| {
            window
                .within(("tabs", panes[0].get()))
                .find(0usize)
                .bounds()
                .center()
        })
        .expect("window");
    let row = center_of(cx, window, [("list", panes[0].get()), ("row", 0)]);
    renders_for_move(cx, window, row);
    let before = child_renders();
    renders_for_move(cx, window, tabs);
    let after = child_renders();
    assert_eq!(after.panes[&panes[0]], before.panes[&panes[0]]);
    assert_eq!(after.panes[&panes[1]], before.panes[&panes[1]]);
    assert_eq!(after.title, before.title);
}

fn selected_hidden_tab(cx: &mut TestAppContext, window: AnyWindowHandle, pane: u64) -> Vec<usize> {
    cx.update_window(window, |_, window, _| {
        let tabs = window.within(("tabs", pane));
        (0..8usize)
            .filter(|index| {
                tabs.try_find(("tab-more-row", *index))
                    .is_some_and(|row| row.selected() == Some(true))
            })
            .collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_hidden_tabs_menu_follows_the_arrow_keys_through_the_cache(cx: &mut TestAppContext) {
    let rig = Rig::open(cx, NARROW);
    let pane = rig.pane.get();
    rig.click(cx, "tabs", "tab-more");
    frame(cx, rig.window);
    let start = selected_hidden_tab(cx, rig.window, pane);
    assert_eq!(start.len(), 1, "one row is selected");
    for step in 1..=2 {
        cx.update_window(rig.window, |_, window, cx| {
            let down = Keystroke::parse("down").expect("keystroke");
            assert!(window.dispatch_keystroke(down, cx));
        })
        .expect("window");
        frame(cx, rig.window);
        let now = selected_hidden_tab(cx, rig.window, pane);
        assert_eq!(now.len(), 1);
        assert_eq!(now[0], start[0] + step, "after {step} presses");
    }
}

fn waiting_label(cx: &mut TestAppContext, window: AnyWindowHandle, pane: u64) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        let waiting = window.try_find(("waiting", pane))?;
        Some(waiting.label().map(str::to_owned).unwrap_or_default())
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_waiting_label_follows_an_offline_folder_through_the_cache(cx: &mut TestAppContext) {
    let (rig, pane) = stalled(cx);
    let pane = pane.get();
    enter_slow(&rig, cx);
    frame(cx, rig.window);
    assert_eq!(waiting_label(cx, rig.window, pane), None);
    wait(cx, 2);
    frame(cx, rig.window);
    let label = waiting_label(cx, rig.window, pane).expect("the waiting view");
    assert!(label.ends_with("is not answering · retrying"), "{label}");
    rig.fs.release();
    rig.settle_until(cx, |view| {
        view.browser.state().active_tab().listing().is_some()
    });
    frame(cx, rig.window);
    assert_eq!(waiting_label(cx, rig.window, pane), None);
}

fn row_text(cx: &mut TestAppContext, window: AnyWindowHandle, pane: u64) -> String {
    cx.update_window(window, |_, window, _| {
        window
            .within(("list", pane))
            .find(("row", 0usize))
            .label()
            .map(str::to_owned)
            .unwrap_or_default()
    })
    .expect("window")
}

#[gpui_kit::test]
fn the_second_pane_shows_its_new_folder_through_the_cache(cx: &mut TestAppContext) {
    let (window, view) = open_two_panes(cx, None);
    let panes = view.read_with(cx, |view, _| view.panes());
    let second = panes[1].get();
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.focus_pane(panes[1]);
            view.focus_core_pane(window, cx);
            cx.notify();
        });
    })
    .expect("window");
    frame(cx, window);
    let before = row_text(cx, window, second);
    assert!(!before.contains("report.txt"), "{before}");
    view.update(cx, |view, cx| {
        let active = view.browser.state().pane(panes[1]).expect("pane").active();
        let docs = active
            .location()
            .join(&rusty_core::ProviderPath::from_text("docs"))
            .expect("docs");
        let _ = view
            .browser
            .command(&rusty_core::Command::PaneNavigate { location: docs });
        cx.notify();
    });
    settle(cx, &view);
    frame(cx, window);
    let after = row_text(cx, window, second);
    assert!(after.contains("report.txt"), "{after}");
}
