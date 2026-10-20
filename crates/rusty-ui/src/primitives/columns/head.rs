use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, DragMoveEvent, ElementId, Empty, Entity, EntityId, FontWeight,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Render,
    RenderOnce, Role, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use rusty_l10n::{t, tr};

use super::set::{ColumnAlign, ColumnEvent, ColumnSet};
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::{self, Theme};

const HANDLE_WIDTH: f32 = 6.;

#[derive(Clone, Copy)]
struct DraggedEdge {
    set: EntityId,
    at: usize,
}

struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(IntoElement)]
pub struct ColumnHead {
    id: ElementId,
    set: Entity<ColumnSet>,
    sorted: Option<(usize, bool)>,
}

impl ColumnHead {
    pub fn new(id: impl Into<ElementId>, set: &Entity<ColumnSet>) -> Self {
        Self {
            id: id.into(),
            set: set.clone(),
            sorted: None,
        }
    }

    #[must_use]
    pub fn sorted(mut self, at: usize, descending: bool) -> Self {
        self.sorted = Some((at, descending));
        self
    }
}

fn update(set: &Entity<ColumnSet>, cx: &mut App, change: impl FnOnce(&mut ColumnSet)) {
    set.update(cx, |set, cx| {
        change(set);
        cx.emit(ColumnEvent::Resized);
        cx.notify();
    });
}

struct Title {
    at: usize,
    text: SharedString,
    align: ColumnAlign,
    sorted: Option<bool>,
}

fn title(t: &Theme, set: Entity<ColumnSet>, title: Title) -> impl IntoElement {
    let Title {
        at,
        text,
        align,
        sorted,
    } = title;
    let arrow = sorted.map(|descending| {
        let glyph = if descending {
            IconName::ArrowDown
        } else {
            IconName::ArrowUp
        };
        icon(glyph, px(10.), t.colors.accent_text)
    });
    let inner = (t.spacing.md - px(HANDLE_WIDTH)).max(px(0.));
    div()
        .id(("column-title", at))
        .role(Role::Button)
        .aria_label(text.clone())
        .test_support()
        .flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .gap(t.spacing.xs)
        .pl(t.spacing.md)
        .pr(inner)
        .cursor_pointer()
        .when(align == ColumnAlign::End, |cell| cell.justify_end())
        .when(sorted.is_some(), |cell| cell.text_color(t.colors.text))
        .child(div().min_w_0().truncate().child(text))
        .children(arrow)
        .on_click(move |_, _, cx| {
            set.update(cx, |_, cx| cx.emit(ColumnEvent::Sort(at)));
        })
}

fn handle(t: &Theme, set: Entity<ColumnSet>, at: usize, label: &str) -> impl IntoElement {
    let accent = t.colors.accent;
    let owner = set.entity_id();
    div()
        .id(("column-handle", at))
        .role(Role::Splitter)
        .aria_label(t!("columns-resize", column = label.to_owned()))
        .test_support()
        .group("column-handle")
        .flex()
        .flex_none()
        .justify_end()
        .w(px(HANDLE_WIDTH))
        .h_full()
        .cursor_col_resize()
        .tooltip(|window, cx| Tooltip::new(tr("columns-fit-hint")).build(window, cx))
        .on_mouse_down(MouseButton::Left, {
            let set = set.clone();
            move |event: &MouseDownEvent, _, cx| {
                let x = event.position.x;
                set.update(cx, |set, _| set.begin_drag(at, x));
            }
        })
        .on_click(move |event, _, cx| {
            if event.click_count() >= 2 {
                update(&set, cx, |set| set.fit(at));
            }
        })
        .on_drag(DraggedEdge { set: owner, at }, |_, _, _, cx| {
            cx.new(|_| DragGhost)
        })
        .child(
            div()
                .w(px(1.))
                .h_full()
                .bg(t.colors.line)
                .group_hover("column-handle", move |style| style.bg(accent)),
        )
}

struct Column<'a> {
    set: &'a Entity<ColumnSet>,
    at: usize,
    sorted: Option<bool>,
}

fn column(t: &Theme, column: Column<'_>, cx: &App) -> impl IntoElement {
    let Column { set, at, sorted } = column;
    let read = set.read(cx);
    let text = read
        .specs
        .get(at)
        .map(|spec| spec.title.clone())
        .unwrap_or_default();
    let align = read.align(at);
    let state = set.clone();
    let owner = set.entity_id();
    read.sized(at)
        .id(("column", at))
        .test_support()
        .flex()
        .h_full()
        .on_drag_move(move |event: &DragMoveEvent<DraggedEdge>, _, cx| {
            let edge = *event.drag(cx);
            if edge.set != owner || edge.at != at {
                return;
            }
            let x = event.event.position.x;
            update(&state, cx, |set| set.drag_to(at, x));
        })
        .child(title(
            t,
            set.clone(),
            Title {
                at,
                text: text.clone(),
                align,
                sorted,
            },
        ))
        .child(handle(t, set.clone(), at, &text))
}

impl RenderOnce for ColumnHead {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let count = self.set.read(cx).specs.len();
        let columns: Vec<_> = (0..count)
            .map(|at| {
                let sorted = self
                    .sorted
                    .filter(|(sorted, _)| *sorted == at)
                    .map(|(_, descending)| descending);
                column(
                    &t,
                    Column {
                        set: &self.set,
                        at,
                        sorted,
                    },
                    cx,
                )
            })
            .collect();
        div()
            .id(self.id)
            .test_support()
            .flex()
            .flex_none()
            .w_full()
            .h(t.spacing.row_height)
            .overflow_hidden()
            .text_size(t.typography.tiny)
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.colors.muted)
            .bg(t.colors.rail)
            .border_b_1()
            .border_color(t.colors.line)
            .children(columns)
    }
}
