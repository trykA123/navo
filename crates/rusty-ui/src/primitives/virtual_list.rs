use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::{Scrollbar, ScrollbarMode, TestSupportExt};
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    ScrollStrategy, SharedString, StatefulInteractiveElement, Styled, UniformListScrollHandle,
    Window, div, uniform_list,
};

use crate::tokens;

type RowRenderer = Rc<dyn Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement>>;

#[derive(Clone, Default)]
pub struct ListScroll(UniformListScrollHandle);

impl ListScroll {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scroll_to(&self, index: usize) {
        self.0.scroll_to_item(index, ScrollStrategy::Nearest);
    }
}

#[derive(IntoElement)]
pub struct VirtualList {
    id: ElementId,
    len: usize,
    scroll: ListScroll,
    label: Option<SharedString>,
    render_rows: RowRenderer,
}

impl VirtualList {
    pub fn new(
        id: impl Into<ElementId>,
        len: usize,
        scroll: &ListScroll,
        render_rows: impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement> + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            len,
            scroll: scroll.clone(),
            label: None,
            render_rows: Rc::new(render_rows),
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

fn wrap_rows(
    rows: Vec<AnyElement>,
    start: usize,
    len: usize,
    row_height: gpui_kit::Pixels,
) -> Vec<AnyElement> {
    rows.into_iter()
        .zip(start..)
        .map(|(row, index)| {
            div()
                .id(("row", index))
                .role(Role::ListItem)
                .aria_position_in_set(index + 1)
                .aria_size_of_set(len)
                .test_support()
                .flex()
                .items_center()
                .w_full()
                .h(row_height)
                .flex_none()
                .child(row)
                .into_any_element()
        })
        .collect()
}

impl RenderOnce for VirtualList {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let row_height = tokens::current(cx).spacing.row_height;
        let len = self.len;
        let render_rows = self.render_rows;
        let rows = uniform_list("rows", len, move |range: Range<usize>, window, cx| {
            let start = range.start;
            wrap_rows(render_rows(range, window, cx), start, len, row_height)
        })
        .track_scroll(&self.scroll.0)
        .size_full();
        let list = div().id(self.id).role(Role::List);
        let list = match self.label {
            Some(label) => list.aria_label(label),
            None => list,
        };
        list.test_support()
            .relative()
            .flex_1()
            .min_h_0()
            .size_full()
            .child(rows)
            .child(
                Scrollbar::vertical(&self.scroll.0)
                    .id("scrollbar")
                    .mode(ScrollbarMode::Always),
            )
    }
}
