use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, RenderOnce, SharedString, Styled, Window, canvas, div,
};

use super::gutter_cell::gutter_cell;
use super::row::{RowLook, row_element};
use super::{CompareEvent, EventHandler, gutter_width};
use crate::browser::RowView;
use crate::primitives::{ListScroll, RowSemantics, VirtualList};
use crate::tokens::{self, Theme};

/// One pane's compare rows. Both panes share one `ListScroll`, so their rows stay lined up.
#[derive(IntoElement)]
pub struct CompareList {
    id: ElementId,
    rows: Arc<[RowView]>,
    scroll: ListScroll,
    selected: Option<usize>,
    is_active: bool,
    empty: Option<SharedString>,
    label: SharedString,
    focus: Option<FocusHandle>,
    on_event: Option<EventHandler>,
    on_scroll: Option<ScrollWatch>,
    gutter: bool,
}

/// The offset last painted, and what to run when a paint finds a different one.
type ScrollWatch = (Rc<Cell<Pixels>>, Rc<dyn Fn(&mut Window, &mut App)>);

impl CompareList {
    pub fn new(id: impl Into<ElementId>, rows: Arc<[RowView]>, scroll: &ListScroll) -> Self {
        Self {
            id: id.into(),
            rows,
            scroll: scroll.clone(),
            selected: None,
            is_active: false,
            empty: None,
            label: SharedString::default(),
            focus: None,
            on_event: None,
            on_scroll: None,
            gutter: false,
        }
    }

    /// The index in `rows` of the selected row.
    #[must_use]
    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    /// Whether this pane holds the keyboard focus.
    #[must_use]
    pub fn active(mut self, is_active: bool) -> Self {
        self.is_active = is_active;
        self
    }

    /// The text shown instead of rows when there are none.
    #[must_use]
    pub fn empty_text(mut self, text: Option<String>) -> Self {
        self.empty = text.map(Into::into);
        self
    }

    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    #[must_use]
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    #[must_use]
    pub fn on_event(
        mut self,
        handler: impl Fn(&CompareEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }

    /// Runs `handler` after a paint finds the scroll offset moved from `seen` (the wheel, keys,
    /// the scrollbar), so the other view over the same offset can repaint. Idle frames run nothing.
    #[must_use]
    pub fn on_scroll(
        mut self,
        seen: Rc<Cell<Pixels>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_scroll = Some((seen, Rc::new(handler)));
        self
    }

    /// Whether this pane draws the gutter column at the trailing edge of its rows: the pane that
    /// shows the compare's left side does, the other one does not.
    #[must_use]
    pub fn gutter(mut self, on: bool) -> Self {
        self.gutter = on;
        self
    }
}

/// A zero-size child that, once the list is painted, reports a changed scroll offset.
fn scroll_watch(scroll: ListScroll, (seen, handler): ScrollWatch) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, (), window, cx| {
            let offset = scroll.offset_y();
            if seen.replace(offset) != offset {
                handler(window, cx);
            }
        },
    )
    .absolute()
    .size_0()
}

struct Rows {
    rows: Arc<[RowView]>,
    selected: Option<usize>,
    is_active: bool,
    on_event: Option<EventHandler>,
    /// The gutter column's width, or `None` in the pane that draws no gutter.
    gutter: Option<Pixels>,
}

impl Rows {
    fn element(&self, index: usize, t: &Theme) -> Option<AnyElement> {
        let row = self.rows.get(index)?;
        let look = RowLook {
            theme: t,
            row,
            is_selected: self.selected == Some(index),
            is_active: self.is_active,
        };
        let on_event = self.on_event.clone();
        let path = row.path.clone();
        let cells = row_element(&look).id("cells").on_mouse_down(
            MouseButton::Left,
            move |event, window, cx| {
                let Some(handler) = &on_event else {
                    return;
                };
                let path = path.clone();
                let event = match event.click_count {
                    0 | 1 => CompareEvent::Select(path),
                    _ => CompareEvent::Open(path),
                };
                handler(&event, window, cx);
            },
        );
        // The gutter column sits past the cells, so it keeps the row's height and its trailing edge.
        let element = match self.gutter {
            Some(width) => div()
                .flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(cells)
                .child(gutter_cell(
                    t,
                    ("gutter", index),
                    width,
                    row.gutter.as_ref(),
                ))
                .into_any_element(),
            None => cells.into_any_element(),
        };
        Some(element)
    }

    fn render(&self, range: Range<usize>, cx: &App) -> Vec<AnyElement> {
        let t = tokens::current(cx);
        range.filter_map(|index| self.element(index, t)).collect()
    }

    fn describe(&self, index: usize) -> RowSemantics {
        RowSemantics {
            label: self.rows.get(index).map(spoken),
            is_selected: Some(self.selected == Some(index)),
            is_active_descendant: self.is_active && self.selected == Some(index),
        }
    }
}

fn spoken(row: &RowView) -> SharedString {
    if row.side.is_none() {
        return navo_l10n::tr("compare-gap").into();
    }
    match row.status.as_str() {
        "" => row.name.clone().into(),
        status => format!("{}, {status}", row.name).into(),
    }
}

fn empty_view(t: &Theme, text: SharedString) -> impl IntoElement {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .p(t.spacing.xxl)
        .text_color(t.colors.muted)
        .child(text)
}

impl RenderOnce for CompareList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::assets::load_mono_fonts(cx);
        let t = tokens::current(cx).clone();
        let fill = || div().flex().flex_col().flex_1().min_w_0().min_h_0();
        let shell = fill()
            .id(self.id)
            .key_context(crate::file_list::LIST_CONTEXT)
            .bg(t.surfaces.card)
            .text_size(t.typography.body);
        // The focused child carries `Compare`, so its keys beat the list's whatever the user binds.
        let focused = fill()
            .id("compare-focus")
            .key_context("Compare")
            .when_some(self.focus, |node, focus| node.track_focus(&focus));
        if let Some(text) = self.empty {
            return shell.child(focused.child(empty_view(&t, text)));
        }
        let rows = Rc::new(Rows {
            rows: self.rows,
            selected: self.selected,
            is_active: self.is_active,
            on_event: self.on_event,
            // The window's width is only known here, and it picks the gutter's narrow or wide size.
            gutter: self
                .gutter
                .then(|| gutter_width(window.viewport_size().width)),
        });
        let described = Rc::clone(&rows);
        let list = VirtualList::new(
            "rows",
            rows.rows.len(),
            &self.scroll,
            move |range, _, cx| rows.render(range, cx),
        )
        .row_height(t.rows.height)
        .label(self.label)
        .describe_rows(move |index| described.describe(index));
        let watch = self.on_scroll.map(|watch| scroll_watch(self.scroll, watch));
        shell.child(focused.child(list)).children(watch)
    }
}
