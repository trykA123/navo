use std::ops::Range;
use std::sync::Arc;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, ElementId, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, RenderOnce, SharedString, Styled, Window, div,
};

use super::row::{RowLook, row_element};
use super::{CompareEvent, EventHandler};
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
}

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
        self.on_event = Some(std::rc::Rc::new(handler));
        self
    }
}

struct Rows {
    rows: Arc<[RowView]>,
    selected: Option<usize>,
    is_active: bool,
    on_event: Option<EventHandler>,
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
        let element = row_element(&look).id("cells").on_mouse_down(
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
        Some(element.into_any_element())
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::assets::load_mono_fonts(cx);
        let t = tokens::current(cx).clone();
        let shell = div()
            .id(self.id)
            .key_context(crate::file_list::list_context(true))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(t.surfaces.card)
            .text_size(t.typography.body)
            .when_some(self.focus, |shell, focus| shell.track_focus(&focus));
        if let Some(text) = self.empty {
            return shell.child(empty_view(&t, text));
        }
        let rows = std::rc::Rc::new(Rows {
            rows: self.rows,
            selected: self.selected,
            is_active: self.is_active,
            on_event: self.on_event,
        });
        let described = std::rc::Rc::clone(&rows);
        let list = VirtualList::new(
            "rows",
            rows.rows.len(),
            &self.scroll,
            move |range, _, cx| rows.render(range, cx),
        )
        .row_height(t.rows.height)
        .label(self.label)
        .describe_rows(move |index| described.describe(index));
        shell.child(list)
    }
}
