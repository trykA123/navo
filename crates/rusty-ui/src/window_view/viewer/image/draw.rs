use std::sync::Arc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{
    Corners, CursorStyle, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, RenderImage,
    ScrollDelta, ScrollWheelEvent, Stateful, canvas, fill, point,
};

use super::ImageView;
use super::zoom::{STEP, Viewport, Zoom};
use crate::prelude::*;
use crate::tokens::Theme;

const CHECK: f32 = 8.;
const WHEEL_ZOOM_PIXELS: f32 = 120.;

#[derive(Clone, Copy)]
struct Checker {
    base: Hsla,
    square: Hsla,
}

fn measured(bounds: Bounds<Pixels>, device: f32) -> Viewport {
    Viewport {
        origin: point(f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
        size: size(f32::from(bounds.size.width), f32::from(bounds.size.height)),
        device,
    }
}

fn placed(zoom: Zoom, viewport: &Viewport) -> Bounds<Pixels> {
    let mut zoom = zoom;
    zoom.clamp(viewport);
    let (origin, shown) = zoom.placement(viewport);
    Bounds::new(
        point(px(origin.x), px(origin.y)),
        size(px(shown.width), px(shown.height)),
    )
}

fn cells(from: f32, to: f32) -> impl Iterator<Item = (i64, f32)> {
    let first = (from / CHECK).floor() as i64;
    let last = (to / CHECK).ceil() as i64;
    (first..last).map(|cell| (cell, cell as f32 * CHECK))
}

fn paint_checker(window: &mut Window, visible: Bounds<Pixels>, checker: Checker) {
    window.paint_quad(fill(visible, checker.base));
    let (left, top) = (f32::from(visible.left()), f32::from(visible.top()));
    let (right, bottom) = (f32::from(visible.right()), f32::from(visible.bottom()));
    for (row, y) in cells(top, bottom) {
        for (_, x) in cells(left, right).filter(|(column, _)| (column + row) % 2 != 0) {
            let cell = Bounds::new(point(px(x), px(y)), size(px(CHECK), px(CHECK)));
            window.paint_quad(fill(cell.intersect(&visible), checker.square));
        }
    }
}

fn paint_picture(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    image_bounds: Bounds<Pixels>,
    look: (Arc<RenderImage>, Checker),
) {
    let (pixels, checker) = look;
    let visible = bounds.intersect(&image_bounds);
    if visible.size.width <= px(0.) || visible.size.height <= px(0.) {
        return;
    }
    paint_checker(window, visible, checker);
    let _ = window.paint_image(bounds, image_bounds, Corners::default(), pixels, 0, false);
}

impl ImageView {
    fn wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let viewport = self.viewport();
        let notches = match event.delta {
            ScrollDelta::Lines(lines) => lines.y,
            ScrollDelta::Pixels(pixels) => f32::from(pixels.y) / WHEEL_ZOOM_PIXELS,
        };
        let anchor = point(
            f32::from(event.position.x) - viewport.origin.x,
            f32::from(event.position.y) - viewport.origin.y,
        );
        self.zoom.zoom_by(STEP.powf(notches), anchor, &viewport);
        cx.notify();
    }

    fn drag_to(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(last) = self.drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            cx.notify();
            return;
        }
        self.drag = Some(event.position);
        let moved = event.position - last;
        self.pan_by(point(f32::from(moved.x), f32::from(moved.y)), cx);
    }

    fn painter(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = self.viewport.clone();
        let entity = cx.entity_id();
        let zoom = self.zoom;
        let pixels = self.picture.pixels.clone();
        let checker = Checker {
            base: t.colors.rail,
            square: t.colors.line,
        };
        canvas(
            move |bounds, window, _| {
                let now = measured(bounds, window.scale_factor());
                if viewport.replace(now) != now {
                    window.on_next_frame(move |_, cx| cx.notify(entity));
                }
                placed(zoom, &now)
            },
            move |bounds, image_bounds, window, _| {
                paint_picture(window, bounds, image_bounds, (pixels.clone(), checker));
            },
        )
        .absolute()
        .size_full()
    }

    pub(super) fn stage(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> ObservedElement<Stateful<Div>> {
        let cursor = if self.drag.is_some() {
            CursorStyle::ClosedHand
        } else {
            CursorStyle::OpenHand
        };
        div()
            .id("image-stage")
            .test_support()
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(t.colors.background)
            .cursor(cursor)
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
                view.wheel(event, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _, cx| {
                    view.drag = Some(event.position);
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, cx| {
                    view.drag = None;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                view.drag_to(event, cx);
            }))
            .child(self.painter(t, cx))
    }
}
