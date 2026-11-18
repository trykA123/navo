//! Holds a file drag back for Shift and Ctrl presses until the pointer moves 6 px.
//! GPUI starts a drag after 2 px, which turns a slightly shaky Shift+click into a self-drop.
//! The gate works in the capture phase of each move, so it never depends on a redraw.

use std::rc::Rc;

use gpui_kit::{
    App, DispatchPhase, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    Styled, canvas,
};

use super::ListContext;

/// Pointer travel in pixels before a Shift or Ctrl press becomes a drag.
pub(super) const THRESHOLD: f64 = 6.;

/// The press position of a Shift or Ctrl press that may not drag yet.
#[derive(Default)]
pub(super) struct Gate {
    origin: Option<Point<Pixels>>,
}

impl Gate {
    /// Closes the gate at a modified left press.
    pub(super) fn close(&mut self, at: Point<Pixels>) {
        self.origin = Some(at);
    }

    /// True while the pointer at `at` is still within `THRESHOLD` of a closed gate's press.
    /// Opens the gate for good once the pointer reaches `THRESHOLD`.
    fn holds(&mut self, at: Point<Pixels>) -> bool {
        let Some(origin) = self.origin else {
            return false;
        };
        let is_near = (at - origin).magnitude() < THRESHOLD;
        if !is_near {
            self.origin = None;
        }
        is_near
    }

    /// Opens the gate at a release or when no button is held any more.
    fn open(&mut self) {
        self.origin = None;
    }
}

impl ListContext {
    /// Closes the gate for a Shift or Ctrl left press at `at`.
    pub(super) fn gate_down(&self, at: Point<Pixels>, cx: &mut App) {
        self.state.update(cx, |state, _| state.grip_gate.close(at));
    }

    /// True when a move to `at` must not reach GPUI's 2 px drag start.
    fn gate_holds(&self, at: Point<Pixels>, cx: &mut App) -> bool {
        self.state.update(cx, |state, _| state.grip_gate.holds(at))
    }

    fn gate_open(&self, cx: &mut App) {
        self.state.update(cx, |state, _| state.grip_gate.open());
    }
}

/// Watches the pointer in the window: a held-back move stops in the capture phase,
/// so GPUI's drag check in the bubble phase never sees it, whatever was drawn last.
pub(super) fn watch(ctx: &Rc<ListContext>) -> impl IntoElement {
    let ctx = ctx.clone();
    canvas(
        |_, _, _| {},
        move |_, (), window, _| {
            let (moved, released) = (ctx.clone(), ctx);
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                if event.pressed_button != Some(MouseButton::Left) {
                    moved.gate_open(cx);
                } else if moved.gate_holds(event.position, cx) {
                    cx.stop_propagation();
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    released.gate_open(cx);
                }
            });
        },
    )
    .absolute()
    .size_0()
}
