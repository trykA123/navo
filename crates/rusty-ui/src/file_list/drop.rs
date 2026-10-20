use std::rc::Rc;

use gpui_kit::{
    App, Div, DragMoveEvent, Entity, ExternalPaths, InteractiveElement, MouseButton, Stateful,
    StatefulInteractiveElement, Styled, Window, div,
};
use rusty_core::PaneId;
use rusty_fs::EntryKind;

use super::row::inset_ring;
use super::{FileList, ListContext, row_index};
use crate::SharedString;
use crate::drag::{DragBadge, FileDrag};
use crate::tokens::Theme;

type DragStartHandler = Rc<dyn Fn(&FileDrag, &mut Window, &mut App) -> Entity<DragBadge>>;
type DropHandler = Rc<dyn Fn(&DropEvent, &mut Window, &mut App)>;
type HoverHandler = Rc<dyn Fn(&Option<DropSpot>, &mut Window, &mut App)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropPayload {
    Internal(FileDrag),
    External(ExternalPaths),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropSpot {
    Row(u64, SharedString),
    Folder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropEvent {
    pub pane: PaneId,
    pub spot: DropSpot,
    pub payload: DropPayload,
}

#[derive(Clone, Default)]
pub(super) struct Wiring {
    pane: Option<PaneId>,
    on_drag_start: Option<DragStartHandler>,
    on_drop: Option<DropHandler>,
    on_hover: Option<HoverHandler>,
}

impl FileList {
    #[must_use]
    pub fn pane(mut self, pane: PaneId) -> Self {
        self.drag.pane = Some(pane);
        self
    }

    #[must_use]
    pub fn on_drag_start(
        mut self,
        handler: impl Fn(&FileDrag, &mut Window, &mut App) -> Entity<DragBadge> + 'static,
    ) -> Self {
        self.drag.on_drag_start = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn on_drop(
        mut self,
        handler: impl Fn(&DropEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.drag.on_drop = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn on_hover(
        mut self,
        handler: impl Fn(&Option<DropSpot>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.drag.on_hover = Some(Rc::new(handler));
        self
    }
}

trait Payload: 'static {
    fn payload(&self) -> DropPayload;
}

impl Payload for FileDrag {
    fn payload(&self) -> DropPayload {
        DropPayload::Internal(self.clone())
    }
}

impl Payload for ExternalPaths {
    fn payload(&self) -> DropPayload {
        DropPayload::External(self.clone())
    }
}

impl ListContext {
    fn accepts(&self) -> bool {
        self.drag.pane.is_some() && self.drag.on_drop.is_some()
    }

    fn hovers(&self) -> bool {
        self.accepts() && self.drag.on_hover.is_some()
    }

    fn name_at(&self, row: u64) -> SharedString {
        self.props
            .rows
            .get(row_index(row))
            .map(|entry| entry.name.to_text().into())
            .unwrap_or_default()
    }

    fn dropped(&self, (spot, payload): (DropSpot, DropPayload), window: &mut Window, cx: &mut App) {
        if let (Some(pane), Some(handler)) = (self.drag.pane, &self.drag.on_drop) {
            let event = DropEvent {
                pane,
                spot,
                payload,
            };
            handler(&event, window, cx);
        }
    }

    fn report_hover(&self, window: &mut Window, cx: &mut App) {
        let spot = self.state.update(cx, |state, _| {
            state.hover_queued = false;
            state.drop_spot.clone()
        });
        if let Some(handler) = &self.drag.on_hover {
            handler(&spot, window, cx);
        }
    }
}

pub(super) fn grip(ctx: &Rc<ListContext>, row: u64) -> Stateful<Div> {
    let grip = div().id("grip");
    let (Some(pane), Some(start)) = (ctx.drag.pane, ctx.drag.on_drag_start.clone()) else {
        return grip;
    };
    let name = ctx.name_at(row);
    let (state, pressed) = (ctx.state.clone(), ctx.state.clone());
    let press_name = name.clone();
    grip.on_mouse_down(MouseButton::Left, move |_, _, cx| {
        let name = press_name.clone();
        pressed.update(cx, |state, _| state.pressed_name = Some(name));
    })
    .on_drag(FileDrag { pane, row, name }, move |drag, _, window, cx| {
        let pressed = state.update(cx, |state, _| {
            state.lasso.swallow();
            state.pressed_name.take()
        });
        let name = pressed.unwrap_or_else(|| drag.name.clone());
        start(
            &FileDrag {
                name,
                ..drag.clone()
            },
            window,
            cx,
        )
    })
}

fn drops<T: Payload, E: InteractiveElement>(el: E, ctx: &Rc<ListContext>, spot: &DropSpot) -> E {
    let (ctx, spot) = (ctx.clone(), spot.clone());
    el.on_drop(move |value: &T, window, cx| {
        ctx.dropped((spot.clone(), value.payload()), window, cx);
    })
}

fn hover_row<T: 'static>(
    ctx: &Rc<ListContext>,
    spot: &DropSpot,
) -> impl Fn(&DragMoveEvent<T>, &mut Window, &mut App) + 'static {
    let (state, spot) = (ctx.state.clone(), spot.clone());
    move |event, _, cx| {
        if event.bounds.contains(&event.event.position) {
            let spot = spot.clone();
            state.update(cx, |state, _| state.drop_spot = Some(spot));
        }
    }
}

fn hover_area<T: 'static>(
    ctx: &Rc<ListContext>,
) -> impl Fn(&DragMoveEvent<T>, &mut Window, &mut App) + 'static {
    let ctx = ctx.clone();
    move |event, window, cx| {
        let spot = event
            .bounds
            .contains(&event.event.position)
            .then_some(DropSpot::Folder);
        let queued = ctx.state.update(cx, |state, _| {
            state.drop_spot = spot;
            std::mem::replace(&mut state.hover_queued, true)
        });
        if !queued {
            let ctx = ctx.clone();
            window.defer(cx, move |window, cx| ctx.report_hover(window, cx));
        }
    }
}

pub(super) fn target(
    cell: Stateful<Div>,
    ctx: &Rc<ListContext>,
    (row, t): (u64, &Theme),
) -> Stateful<Div> {
    let is_folder = ctx
        .props
        .rows
        .get(row_index(row))
        .is_some_and(|entry| entry.kind == EntryKind::Folder);
    if !is_folder || !ctx.accepts() {
        return cell;
    }
    let ring = t.colors.accent;
    let spot = DropSpot::Row(row, ctx.name_at(row));
    let cell = cell
        .drag_over::<FileDrag>(move |style, _, _, _| style.shadow(inset_ring(ring)))
        .drag_over::<ExternalPaths>(move |style, _, _, _| style.shadow(inset_ring(ring)));
    let cell = drops::<ExternalPaths, _>(drops::<FileDrag, _>(cell, ctx, &spot), ctx, &spot);
    if !ctx.hovers() {
        return cell;
    }
    cell.on_drag_move(hover_row::<FileDrag>(ctx, &spot))
        .on_drag_move(hover_row::<ExternalPaths>(ctx, &spot))
}

pub(super) fn background(area: Div, ctx: &Rc<ListContext>) -> Div {
    let area = if ctx.accepts() {
        let spot = DropSpot::Folder;
        drops::<ExternalPaths, _>(drops::<FileDrag, _>(area, ctx, &spot), ctx, &spot)
    } else {
        area
    };
    if !ctx.hovers() {
        return area;
    }
    area.on_drag_move(hover_area::<FileDrag>(ctx))
        .on_drag_move(hover_area::<ExternalPaths>(ctx))
}
