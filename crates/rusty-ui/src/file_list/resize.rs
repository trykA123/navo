use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    App, AppContext as _, Context, Div, DragMoveEvent, Empty, Entity, EntityId, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, ParentElement, Pixels, Render, Role, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use rusty_l10n::{t, tr};

use super::header::{index_of, label};
use super::widths::{ColumnWidths, UserWidths, clamp_width, min_width};
use super::{FileListAction, KnownWidth, ListContext, ListParts, ListState, SortBy, column_drop};
use crate::primitives::Tooltip;
use crate::tokens::{self, Theme};

const HANDLE_WIDTH: f32 = 6.;

#[derive(Clone, Copy, Debug)]
pub(super) struct Resize {
    by: SortBy,
    from_x: Pixels,
    from_width: Pixels,
    room: Option<Pixels>,
    width: Option<Pixels>,
}

#[derive(Clone, Copy)]
struct ResizeEdge {
    list: EntityId,
    by: SortBy,
}

struct Ghost;

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

pub(super) fn shown(
    (state, known): (&Entity<ListState>, Option<&KnownWidth>),
    (parts, user): (ListParts, UserWidths),
    window: &Window,
    cx: &mut App,
) -> (ListParts, ColumnWidths) {
    let t = tokens::current(cx).clone();
    let widths = live(state, ColumnWidths::resolve(user, &t, window, cx), cx);
    let width = column_drop::list_width(state, known, cx);
    (column_drop::fit(width, parts, (&t, widths)), widths)
}

fn live(state: &Entity<ListState>, widths: ColumnWidths, cx: &App) -> ColumnWidths {
    match state.read(cx).resize {
        Some(Resize {
            by,
            width: Some(width),
            ..
        }) => widths.with(by, width),
        _ => widths,
    }
}

impl ListContext {
    fn room_for(&self, by: SortBy, cx: &App) -> Option<Pixels> {
        let width = self.state.read(cx).list_bounds?.size.width;
        let t = tokens::current(cx);
        let others = column_drop::needed(t, self.parts, self.widths.with(by, px(0.)));
        Some(width - others)
    }

    fn begin_resize(&self, by: SortBy, x: Pixels, cx: &mut App) {
        let resize = Resize {
            by,
            from_x: x,
            from_width: self.widths.of(by).unwrap_or_default(),
            room: self.room_for(by, cx),
            width: None,
        };
        self.state
            .update(cx, |state, _| state.resize = Some(resize));
    }

    fn resize_to(&self, by: SortBy, x: Pixels, cx: &mut App) {
        let t = tokens::current(cx).clone();
        self.state.update(cx, |state, cx| {
            let Some(resize) = state.resize.as_mut().filter(|resize| resize.by == by) else {
                return;
            };
            let wanted = clamp_width(&t, resize.from_width - (x - resize.from_x));
            let width = resize
                .room
                .map_or(wanted, |room| wanted.min(room.max(min_width(&t))));
            if resize.width != Some(width) {
                resize.width = Some(width);
                cx.notify();
            }
        });
    }

    fn end_resize(&self, window: &mut Window, cx: &mut App) {
        let done = self.state.update(cx, |state, cx| {
            let done = state.resize.take();
            if done.is_some() {
                cx.notify();
            }
            done
        });
        if let Some(Resize {
            by,
            from_width,
            width: Some(width),
            ..
        }) = done
            && width != from_width
        {
            self.act(FileListAction::ResizeColumn { by, width }, window, cx);
        }
    }

    fn fit_column(&self, by: SortBy, window: &mut Window, cx: &mut App) {
        self.act(
            FileListAction::ResizeColumn { by, width: px(0.) },
            window,
            cx,
        );
    }
}

pub(super) fn handle(t: &Theme, by: SortBy, ctx: &Rc<ListContext>) -> impl IntoElement + use<> {
    let accent = t.colors.accent;
    let zone = div()
        .id(("column-handle", index_of(by)))
        .role(Role::Splitter)
        .aria_label(t!("columns-resize", column = label(by)))
        .group("column-handle")
        .absolute()
        .top_0()
        .bottom_0()
        .left(-(t.spacing.md + px(HANDLE_WIDTH)) / 2.)
        .w(px(HANDLE_WIDTH))
        .flex()
        .justify_center()
        .occlude()
        .cursor_col_resize()
        .tooltip(|window, cx| Tooltip::new(tr("columns-fit-hint")).build(window, cx));
    listen(zone, by, ctx).test_support().child(
        div()
            .w(px(1.))
            .h_full()
            .group_hover("column-handle", move |style| style.bg(accent)),
    )
}

fn listen(zone: Stateful<Div>, by: SortBy, ctx: &Rc<ListContext>) -> Stateful<Div> {
    let edge = ResizeEdge {
        list: ctx.state.entity_id(),
        by,
    };
    let (down, moved, up, out, click) = (
        ctx.clone(),
        ctx.clone(),
        ctx.clone(),
        ctx.clone(),
        ctx.clone(),
    );
    zone.on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
        down.begin_resize(by, event.position.x, cx);
    })
    .on_drag(edge, |_, _, _, cx| cx.new(|_| Ghost))
    .on_drag_move(move |event: &DragMoveEvent<ResizeEdge>, _, cx| {
        let dragged = *event.drag(cx);
        if dragged.list == edge.list && dragged.by == by {
            moved.resize_to(by, event.event.position.x, cx);
        }
    })
    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
        up.end_resize(window, cx)
    })
    .on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
        out.end_resize(window, cx);
    })
    .on_click(move |event, window, cx| {
        if event.click_count() >= 2 {
            click.fit_column(by, window, cx);
        }
    })
}
