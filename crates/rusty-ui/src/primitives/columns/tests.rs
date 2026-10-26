use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Context, ElementId, Entity, IntoElement, Pixels, Render,
    Subscription, TestAppContext, Window, point, px,
};

use super::{ColumnAlign, ColumnEvent, ColumnHead, ColumnSet, ColumnSpec};
use crate::primitives::tests::open;

fn spec(id: &'static str, (min_chars, max_chars): (usize, usize), flex: bool) -> ColumnSpec {
    ColumnSpec {
        id: id.into(),
        title: id.into(),
        align: ColumnAlign::Start,
        min_chars,
        max_chars,
        flex,
    }
}

fn specs() -> Vec<ColumnSpec> {
    vec![
        spec("name", (12, 80), true),
        spec("at", (6, 24), false),
        spec("size", (4, 30), false),
    ]
}

fn row(cells: [&str; 3]) -> Vec<String> {
    cells.map(str::to_owned).to_vec()
}

struct Table {
    set: Entity<ColumnSet>,
    events: Rc<RefCell<Vec<ColumnEvent>>>,
    _events: Subscription,
}

impl Render for Table {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ColumnHead::new("head", &self.set).sorted(2, true)
    }
}

fn table(rows: Vec<Vec<String>>) -> impl FnOnce(&mut Window, &mut gpui_kit::App) -> Table {
    move |window, cx| {
        let set = cx.new(|cx| {
            let mut set = ColumnSet::new(specs(), window, cx);
            set.fit_content(rows);
            set
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        let subscription = cx.subscribe(&set, move |_, event: &ColumnEvent, _| {
            seen.borrow_mut().push(*event);
        });
        Table {
            set,
            events,
            _events: subscription,
        }
    }
}

fn sample() -> Vec<Vec<String>> {
    vec![
        row(["main", "0x0800_0000_0000_0000_0000_0000", "12"]),
        row(["init", "0x10", "1"]),
    ]
}

fn chars(set: &ColumnSet, count: usize) -> Pixels {
    set.char_width * count as f32 + set.padding
}

fn width(view: &Entity<Table>, at: usize, cx: &mut TestAppContext) -> Pixels {
    view.read_with(cx, |view, cx| view.set.read(cx).width(at))
}

fn bounds(
    window: AnyWindowHandle,
    id: impl Into<ElementId>,
    cx: &mut TestAppContext,
) -> Bounds<Pixels> {
    let id = id.into();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("head").find(id).bounds()
    })
    .expect("window")
}

fn drag_handle(window: AnyWindowHandle, at: usize, by: f32, cx: &mut TestAppContext) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let from = window
            .within("head")
            .find(("column-handle", at))
            .bounds()
            .center();
        window.drag(from, point(from.x + px(by), from.y), cx);
    })
    .expect("window");
}

fn near(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(1.)
}

#[gpui_kit::test]
fn content_widths_clamp_to_the_spec(cx: &mut TestAppContext) {
    let (_, view) = open(cx, table(sample()));
    view.read_with(cx, |view, cx| {
        let set = view.set.read(cx);
        assert_eq!(set.width(1), chars(set, 24));
        assert_eq!(set.width(2), chars(set, 6));
        assert_eq!(set.width(0), chars(set, 12));
    });
}

#[gpui_kit::test]
fn only_the_first_200_rows_count(cx: &mut TestAppContext) {
    let mut rows = vec![row(["a", "b", "c"]); 200];
    rows.push(row(["a", "b", "123456789012"]));
    assert_eq!(rows.len(), 201);
    let (_, view) = open(cx, table(rows));
    view.read_with(cx, |view, cx| {
        let set = view.set.read(cx);
        assert_eq!(set.width(2), chars(set, 6));
    });
}

#[gpui_kit::test]
fn the_flex_column_takes_the_rest(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(sample()));
    let fixed = width(&view, 1, cx) + width(&view, 2, cx);
    let name = bounds(window, ("column", 0usize), cx);
    assert!(near(name.size.width, px(800.) - fixed), "name {name:?}");
}

#[gpui_kit::test]
fn the_flex_column_keeps_its_minimum(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(sample()));
    view.update(cx, |view, cx| {
        view.set.update(cx, |set, _| {
            set.specs[2].max_chars = 200;
            set.set_width(2, px(790.));
        });
    });
    let minimum = view.read_with(cx, |view, cx| {
        let set = view.set.read(cx);
        chars(set, 12)
    });
    let name = bounds(window, ("column", 0usize), cx);
    assert!(near(name.size.width, minimum), "name {name:?}");
}

#[gpui_kit::test]
fn a_drag_of_40_px_widens_one_column(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(vec![row(["a", "b", "c"])]));
    let before = [0, 1].map(|at| width(&view, at, cx));
    drag_handle(window, 1, 40., cx);
    assert!(near(width(&view, 1, cx), before[1] + px(40.)));
    assert_eq!(width(&view, 0, cx), before[0]);
    let events = view.read_with(cx, |view, _| view.events.borrow().clone());
    assert!(events.contains(&ColumnEvent::Resized), "{events:?}");
}

#[gpui_kit::test]
fn dragging_the_flex_edge_resizes_the_next_column(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(vec![row(["a", "b", "c"])]));
    let next = width(&view, 1, cx);
    let edge = bounds(window, ("column", 0usize), cx).right();
    drag_handle(window, 0, -20., cx);
    assert!(near(width(&view, 1, cx), next + px(20.)));
    let moved = bounds(window, ("column", 0usize), cx).right();
    assert!(near(moved, edge - px(20.)), "edge {moved:?}");
}

#[gpui_kit::test]
fn widening_a_column_after_the_flex_shrinks_its_neighbour(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(sample()));
    view.update(cx, |view, cx| {
        view.set.update(cx, |set, _| {
            set.set_width(1, px(0.));
            set.set_width(2, px(200.));
        });
    });
    let edge = bounds(window, ("column", 1usize), cx).right();
    drag_handle(window, 1, 30., cx);
    assert!(near(width(&view, 2, cx), px(170.)));
    let moved = bounds(window, ("column", 1usize), cx).right();
    assert!(near(moved, edge + px(30.)), "edge {moved:?}");
}

#[gpui_kit::test]
fn a_double_click_fits_the_column(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(sample()));
    let fitted = width(&view, 2, cx);
    drag_handle(window, 2, 60., cx);
    assert!(width(&view, 2, cx) > fitted);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("head")
            .double_click(("column-handle", 2usize), cx);
    })
    .expect("window");
    assert_eq!(width(&view, 2, cx), fitted);
}

#[gpui_kit::test]
fn reset_restores_the_initial_widths(cx: &mut TestAppContext) {
    let (_, view) = open(cx, table(sample()));
    let before = [0, 1, 2].map(|at| width(&view, at, cx));
    view.update(cx, |view, cx| {
        view.set.update(cx, |set, _| {
            set.set_width(0, px(400.));
            set.set_width(1, px(10.));
            set.reset();
        });
    });
    assert_eq!([0, 1, 2].map(|at| width(&view, at, cx)), before);
}

#[gpui_kit::test]
fn set_width_stays_within_the_spec(cx: &mut TestAppContext) {
    let (_, view) = open(cx, table(sample()));
    view.update(cx, |view, cx| {
        view.set.update(cx, |set, _| {
            set.set_width(1, px(1.));
            assert_eq!(set.width(1), chars(set, 6));
            set.set_width(1, px(5000.));
            assert_eq!(set.width(1), chars(set, 24));
        });
    });
}

#[gpui_kit::test]
fn a_title_click_emits_sort(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(sample()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("head").click(("column-title", 1usize), cx);
    })
    .expect("window");
    let events = view.read_with(cx, |view, _| view.events.borrow().clone());
    assert_eq!(events, vec![ColumnEvent::Sort(1)]);
}

#[gpui_kit::test]
fn a_handle_click_does_not_sort(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(sample()));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.within("head").click(("column-handle", 1usize), cx);
    })
    .expect("window");
    let events = view.read_with(cx, |view, _| view.events.borrow().clone());
    assert!(!events.contains(&ColumnEvent::Sort(1)), "{events:?}");
}

#[gpui_kit::test]
fn the_char_width_is_measured_or_falls_back(cx: &mut TestAppContext) {
    let (_, view) = open(cx, table(sample()));
    let measured = view.read_with(cx, |view, cx| view.set.read(cx).char_width);
    assert!(measured > px(0.) && measured < px(20.), "{measured:?}");
}

#[gpui_kit::test]
fn text_chars_follow_the_shown_width(cx: &mut TestAppContext) {
    let (window, view) = open(cx, table(sample()));
    let name = bounds(window, ("column", 0usize), cx);
    view.read_with(cx, |view, cx| {
        let set = view.set.read(cx);
        let shown = (name.size.width - set.padding) / set.char_width;
        assert_eq!(set.text_chars(0), shown.floor() as usize);
        let fixed = (set.width(1) - set.padding) / set.char_width;
        assert_eq!(set.text_chars(1), fixed.floor() as usize);
        let named = (name.size.width - set.padding) / set.name_char_width;
        assert_eq!(set.name_chars(0), named.floor() as usize);
    });
}

#[gpui_kit::test]
fn a_second_set_reuses_the_measured_widths(cx: &mut TestAppContext) {
    let (_, first) = open(cx, table(sample()));
    let (_, second) = open(cx, table(sample()));
    let widths = |view: &Entity<Table>, cx: &mut TestAppContext| {
        view.read_with(cx, |view, cx| {
            let set = view.set.read(cx);
            (set.char_width, set.name_char_width)
        })
    };
    assert_eq!(widths(&first, cx), widths(&second, cx));
    let cached = cx.update(|cx| {
        cx.try_global::<super::set::MeasuredWidths>()
            .map(|c| c.0.len())
    });
    assert_eq!(cached, Some(2));
}
