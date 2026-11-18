//! Lifts the rows above the modifier-hint drawer, and holds that offset while the button that
//! started a press is down, so the press and its release always land on the same row.

use std::rc::Rc;

use gpui_kit::{
    App, Bounds, DispatchPhase, Entity, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Styled, Window, canvas, div, point, px,
};

use super::{FileList, ListContext, ListState, row_index, row_number};
use crate::modifier_hints::{Cover, rows_to_lift};

impl FileList {
    #[must_use]
    pub fn lift_above(mut self, cover: Option<Cover>) -> Self {
        self.cover = cover;
        self
    }
}

/// Returns the lift in pixels for the cursor row under `cover`; the last lift while a button is
/// held. An inactive window lets a held lift go, since its release may never arrive.
pub(super) fn lift_for(
    state: &Entity<ListState>,
    (cursor, cover): (u64, Option<Cover>),
    row_height: Pixels,
    (window, cx): (&Window, &mut App),
) -> Pixels {
    let is_held = state.read(cx).lift_held.is_some();
    if is_held && window.is_window_active() {
        return state.read(cx).lift;
    }
    if is_held {
        state.update(cx, |state, _| state.lift_held = None);
    }
    let lift = cover.map_or(px(0.), |cover| {
        let state = state.read(cx);
        let Some(list) = state.list_bounds else {
            return px(0.);
        };
        let top = row_number(state.scroll.top_row(row_height));
        let Some(above) = cursor.checked_sub(top) else {
            return px(0.);
        };
        let bottom = list.origin.y + row_height * (above + 1) as f32;
        let rows = rows_to_lift(bottom, cover, row_height).min(row_index(above));
        row_height * rows as f32
    });
    state.update(cx, |state, _| state.lift = lift);
    lift
}

pub(super) fn lifted(list: impl IntoElement, lift: Pixels) -> impl IntoElement {
    div()
        .relative()
        .top(-lift)
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .size_full()
        .child(list)
}

pub(super) fn unshifted(bounds: Bounds<Pixels>, lift: Pixels) -> Bounds<Pixels> {
    Bounds {
        origin: point(bounds.origin.x, bounds.origin.y + lift),
        size: bounds.size,
    }
}

/// Holds the lift for `button` unless another button already holds it.
fn hold_lift(ctx: &ListContext, button: MouseButton, cx: &mut App) {
    ctx.state.update(cx, |state, _| {
        state.lift_held.get_or_insert(button);
    });
}

/// Lets the lift go when `button` holds it, or for any button when `button` is `None`;
/// redraws once it is let go.
fn release_lift(ctx: &ListContext, button: Option<MouseButton>, window: &mut Window, cx: &mut App) {
    let was_held = ctx.state.update(cx, |state, _| {
        if button.is_none() || state.lift_held == button {
            state.lift_held.take()
        } else {
            None
        }
    });
    if was_held.is_some() {
        window.refresh();
    }
}

/// Watches every button in the window: a press holds the lift, the release of the button
/// that started it or a move with no button held lets it go.
pub(super) fn hold(ctx: &Rc<ListContext>) -> impl IntoElement {
    let ctx = ctx.clone();
    canvas(
        |_, _, _| {},
        move |_, (), window, _| {
            let (pressed, moved, released) = (ctx.clone(), ctx.clone(), ctx);
            window.on_mouse_event(move |event: &MouseDownEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    hold_lift(&pressed, event.button, cx);
                }
            });
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && event.pressed_button.is_none() {
                    release_lift(&moved, None, window, cx);
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    release_lift(&released, Some(event.button), window, cx);
                }
            });
        },
    )
    .absolute()
    .size_0()
}
