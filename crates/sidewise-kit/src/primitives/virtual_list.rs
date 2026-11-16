use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::{Scrollbar, ScrollbarHandle, ScrollbarMode, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Role, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, UniformListScrollHandle, Window, div, px, uniform_list,
};

use crate::tokens;

type RowRenderer = Rc<dyn Fn(Range<usize>, &mut Window, &mut App) -> Vec<AnyElement>>;
type RowDescriber = Rc<dyn Fn(usize) -> RowSemantics>;

#[derive(Clone, Default)]
pub struct ListScroll(UniformListScrollHandle);

impl ListScroll {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scroll_to(&self, index: usize) {
        self.0.scroll_to_item(index, ScrollStrategy::Nearest);
    }

    /// Scrolls so row `index` sits in the middle of the list, as far as the ends allow.
    pub fn scroll_to_center(&self, index: usize) {
        self.0.scroll_to_item(index, ScrollStrategy::Center);
    }

    /// Centres row `index` even when it is already visible: a wrapped change spans many items,
    /// so its first item on screen is not the change on screen.
    pub fn scroll_to_center_strict(&self, index: usize) {
        self.0.scroll_to_item_strict(index, ScrollStrategy::Center);
    }

    pub fn scroll_to_top(&self, index: usize) {
        self.0.scroll_to_item_strict(index, ScrollStrategy::Top);
    }

    pub fn offset_y(&self) -> Pixels {
        self.0.0.borrow().base_handle.offset().y
    }

    /// Sets the vertical offset (zero or below) now, not at the next layout, so a read right
    /// after sees it.
    pub fn set_offset_y(&self, y: Pixels) {
        let handle = &self.0.0.borrow().base_handle;
        handle.set_offset(gpui_kit::point(handle.offset().x, y));
    }

    /// The list's height in the last layout; zero before the first one.
    pub fn viewport_height(&self) -> Pixels {
        let state = self.0.0.borrow();
        state.last_item_size.map_or(px(0.), |size| size.item.height)
    }

    /// The list's bounds in the last layout.
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.0.0.borrow().base_handle.bounds()
    }

    pub fn is_at_bottom(&self) -> bool {
        let state = self.0.0.borrow();
        let Some(size) = state.last_item_size else {
            return false;
        };
        let max = size.contents.height - size.item.height;
        max > px(0.) && -state.base_handle.offset().y >= max - px(0.5)
    }

    pub fn top_row(&self, row_height: Pixels) -> usize {
        if row_height <= px(0.) {
            return 0;
        }
        let offset = self.0.0.borrow().base_handle.offset();
        let rows = (-offset.y / row_height).floor();
        if rows.is_finite() && rows > 0. {
            rows as usize
        } else {
            0
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowSemantics {
    pub label: Option<SharedString>,
    pub is_selected: Option<bool>,
    pub is_active_descendant: bool,
}

#[derive(IntoElement)]
pub struct VirtualList {
    id: ElementId,
    len: usize,
    scroll: ListScroll,
    label: Option<SharedString>,
    focus: Option<FocusHandle>,
    render_rows: RowRenderer,
    describe_rows: Option<RowDescriber>,
    row_height: Option<Pixels>,
    bar: Option<AnyElement>,
    axis_locked: bool,
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
            focus: None,
            render_rows: Rc::new(render_rows),
            describe_rows: None,
            row_height: None,
            bar: None,
            axis_locked: false,
        }
    }

    /// Draws the scrollbar over `handle` instead of the list's own, so it can span rows the
    /// list does not hold.
    #[must_use]
    pub fn scrollbar<H: ScrollbarHandle + Clone>(mut self, handle: &H) -> Self {
        self.bar = Some(Self::bar(handle));
        self
    }

    fn bar<H: ScrollbarHandle + Clone>(handle: &H) -> AnyElement {
        Scrollbar::vertical(handle)
            .id("scrollbar")
            .mode(ScrollbarMode::Always)
            .into_any_element()
    }

    /// Keeps the wheel on its own axis: a sideways delta (Shift+wheel, a touchpad swipe) no longer
    /// scrolls the rows, and a touchpad gesture stays on the axis it started on.
    #[must_use]
    pub fn axis_locked(mut self) -> Self {
        self.axis_locked = true;
        self
    }

    #[must_use]
    pub fn row_height(mut self, height: Pixels) -> Self {
        self.row_height = Some(height);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    pub fn describe_rows(mut self, describe: impl Fn(usize) -> RowSemantics + 'static) -> Self {
        self.describe_rows = Some(Rc::new(describe));
        self
    }
}

struct RowFrame {
    start: usize,
    len: usize,
    row_height: Pixels,
    describe: Option<RowDescriber>,
}

fn wrap_rows(rows: Vec<AnyElement>, frame: &RowFrame) -> Vec<AnyElement> {
    rows.into_iter()
        .zip(frame.start..)
        .map(|(row, index)| {
            let semantics = frame
                .describe
                .as_ref()
                .map(|describe| describe(index))
                .unwrap_or_default();
            div()
                .id(("row", index))
                .role(Role::ListItem)
                .aria_position_in_set(index + 1)
                .aria_size_of_set(frame.len)
                .when_some(semantics.label, |row, label| row.aria_label(label))
                .when_some(semantics.is_selected, |row, is_selected| {
                    row.aria_selected(is_selected)
                })
                .when(semantics.is_active_descendant, |row| {
                    row.aria_active_descendant()
                })
                .test_support()
                .flex()
                .items_center()
                .w_full()
                .h(frame.row_height)
                .flex_none()
                .child(row)
                .into_any_element()
        })
        .collect()
}

impl RenderOnce for VirtualList {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let row_height = self
            .row_height
            .unwrap_or(tokens::current(cx).spacing.row_height);
        let len = self.len;
        let render_rows = self.render_rows;
        let describe = self.describe_rows;
        let mut rows = uniform_list("rows", len, move |range: Range<usize>, window, cx| {
            let frame = RowFrame {
                start: range.start,
                len,
                row_height,
                describe: describe.clone(),
            };
            wrap_rows(render_rows(range, window, cx), &frame)
        })
        .track_scroll(&self.scroll.0)
        .size_full();
        if self.axis_locked {
            rows.style().restrict_scroll_to_axis = Some(true);
        }
        let list = div().id(self.id).role(Role::List);
        let list = match self.label {
            Some(label) => list.aria_label(label),
            None => list,
        };
        list.test_support()
            .when_some(self.focus, |list, focus| list.track_focus(&focus))
            .relative()
            .flex_1()
            .min_h_0()
            .size_full()
            .child(rows)
            .child(self.bar.unwrap_or_else(|| Self::bar(&self.scroll.0)))
    }
}
