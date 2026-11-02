use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement, Render, Role, Styled, TestAppContext, Window,
    div,
};

use super::open;
use crate::primitives::{ListScroll, VirtualList};

const ROWS: usize = 100_000;

struct Rows {
    scroll: ListScroll,
    widest_range: Rc<Cell<usize>>,
}

impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let widest = self.widest_range.clone();
        div().size_full().flex().flex_col().child(
            VirtualList::new("files", ROWS, &self.scroll, move |range, _, _| {
                widest.set(widest.get().max(range.len()));
                range
                    .map(|index| div().child(format!("row {index}")).into_any_element())
                    .collect::<Vec<AnyElement>>()
            })
            .label("Generated rows"),
        )
    }
}

fn rows() -> Rows {
    Rows {
        scroll: ListScroll::new(),
        widest_range: Rc::new(Cell::new(0)),
    }
}

#[gpui_kit::test]
fn a_hundred_thousand_rows_render_only_the_visible_ones(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| rows());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.find("files");
        assert_eq!(list.role(), Some(Role::List));
        assert_eq!(list.label(), Some("Generated rows"));
        let list = window.within("files");
        assert!(list.find(("row", 0usize)).visible());
        assert_eq!(list.find(("row", 0usize)).role(), Some(Role::ListItem));
        assert!(list.try_find(("row", ROWS - 1)).is_none());
    })
    .expect("window");
    let widest = view.read_with(cx, |view, _| view.widest_range.get());
    assert!(
        widest > 0 && widest <= 40,
        "rendered {widest} rows in one pass"
    );
}

#[gpui_kit::test]
fn scroll_to_reaches_the_last_row(cx: &mut TestAppContext) {
    let (window, view) = open(cx, |_, _| rows());
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    view.read_with(cx, |view, _| view.scroll.scroll_to(ROWS - 1));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let last = window.within("files").find(("row", ROWS - 1));
        assert!(last.visible(), "row {} is not on screen", ROWS - 1);
        assert!(window.within("files").try_find(("row", 0usize)).is_none());
    })
    .expect("window");
}
