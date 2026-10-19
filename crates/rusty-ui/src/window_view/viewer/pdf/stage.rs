use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, ScrollDelta,
    ScrollWheelEvent, Size, Stateful, StyledImage, canvas, img, point,
};
use rusty_pdf::protocol::CharBox;

use super::PdfView;
use super::doc::Doc;
use super::layout::STEP;
use super::text::rects;
use crate::prelude::*;
use crate::tokens::{Mode, Theme};

const WHEEL_LINE: f32 = 48.;
const DARK_PAGE: f32 = 0.9;
const WHEEL_ZOOM_PIXELS: f32 = 120.;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct Measured {
    pub origin: Point<f32>,
    pub size: Size<f32>,
    pub device: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Paint {
    Hit,
    Current,
    Selected,
}

fn paint_color(t: &Theme, paint: Paint) -> Hsla {
    match paint {
        Paint::Hit => t.marks.ticks[1].opacity(0.35),
        Paint::Current => t.colors.accent.opacity(0.35),
        Paint::Selected => t.colors.selection.opacity(0.6),
    }
}

fn mark(t: &Theme, scale: f32, bounds: CharBox, paint: Paint) -> Div {
    div()
        .absolute()
        .left(px(bounds.left * scale - 1.))
        .top(px(bounds.top * scale - 1.))
        .w(px((bounds.right - bounds.left) * scale + 2.))
        .h(px((bounds.bottom - bounds.top) * scale + 2.))
        .rounded(t.radii.xs)
        .bg(paint_color(t, paint))
        .when(paint == Paint::Current, |mark| {
            mark.border_2().border_color(t.colors.accent)
        })
}

fn marks_on(t: &Theme, doc: &Doc, page: u32) -> Vec<AnyElement> {
    let Some(text) = doc.boxes.get(page) else {
        return Vec::new();
    };
    let scale = doc.scale();
    let current = doc.hits.current().filter(|hit| hit.page == page);
    let mut shown: Vec<AnyElement> = Vec::new();
    for hit in doc.hits.on(page) {
        let is_current = current.is_some_and(|current| current.start == hit.start);
        let paint = if is_current {
            Paint::Current
        } else {
            Paint::Hit
        };
        for bounds in rects(text, hit.start, hit.len) {
            let shown_mark = mark(t, scale, bounds, paint);
            shown.push(if is_current {
                shown_mark
                    .id("pdf-hit-current")
                    .test_support()
                    .into_any_element()
            } else {
                shown_mark.into_any_element()
            });
        }
    }
    if let Some(selection) = doc.selection.filter(|selection| selection.page == page) {
        let start = selection.from.min(selection.to);
        let len = selection.from.abs_diff(selection.to) + 1;
        let marks = rects(text, start, len).into_iter();
        shown
            .extend(marks.map(|bounds| mark(t, scale, bounds, Paint::Selected).into_any_element()));
    }
    shown
}

fn page_state(doc: &Doc, page: u32) -> String {
    let number = page + 1;
    match (doc.broken.get(&page), doc.shots.get(page)) {
        (Some(error), _) => format!("Page {number} was not drawn: {error}"),
        (None, Some(_)) => format!("Page {number}"),
        (None, None) => format!("Page {number}, drawing"),
    }
}

fn sheet(t: &Theme, doc: &Doc, page: u32) -> AnyElement {
    let scale = doc.scale();
    let size = doc.pages.size(page);
    let top = doc.pages.top(page, scale) - doc.camera.scroll.y;
    let left = doc.camera.page_left(&doc.pages, page);
    let label = page_state(doc, page);
    let broken = doc
        .broken
        .get(&page)
        .map(|error| format!("This page was not drawn: {error}"));
    div()
        .id(("pdf-page", page as usize))
        .aria_label(label)
        .test_support()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(size.width * scale))
        .h(px(size.height * scale))
        .bg(t.surfaces.tile)
        .shadow(t.shadows.card.clone())
        .when_some(doc.shots.get(page), |sheet, shot| {
            sheet.child(
                img(shot.image.clone())
                    .absolute()
                    .size_full()
                    .object_fit(ObjectFit::Fill)
                    .when(t.mode == Mode::Dark, |page| page.opacity(DARK_PAGE)),
            )
        })
        .children(marks_on(t, doc, page))
        .when_some(broken, |sheet, message| {
            sheet.child(
                div()
                    .p(t.spacing.xl)
                    .text_size(t.typography.small)
                    .text_color(t.colors.warn)
                    .child(message),
            )
        })
        .into_any_element()
}

impl PdfView {
    pub(super) fn sync_viewport(&mut self) {
        let measured = self.measured.get();
        let Some(doc) = self.doc_mut() else {
            return;
        };
        let camera = &mut doc.camera;
        if camera.viewport == measured.size && camera.device == measured.device.max(1.) {
            return;
        }
        let anchor = doc.pages.anchor(camera.scroll.y, camera.scale(&doc.pages));
        camera.viewport = measured.size;
        camera.device = measured.device.max(1.);
        camera.keep_anchor(&doc.pages, anchor);
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let (delta, notches) = match event.delta {
            ScrollDelta::Lines(lines) => {
                (point(lines.x * WHEEL_LINE, lines.y * WHEEL_LINE), lines.y)
            }
            ScrollDelta::Pixels(pixels) => {
                let pixels = point(f32::from(pixels.x), f32::from(pixels.y));
                (pixels, pixels.y / WHEEL_ZOOM_PIXELS)
            }
        };
        if event.modifiers.control || event.modifiers.platform {
            let pointer = self.local(event.position);
            return self.zoom_by(STEP.powf(notches), Some(pointer), cx);
        }
        let delta = if event.modifiers.shift {
            point(-delta.y, -delta.x)
        } else {
            point(-delta.x, -delta.y)
        };
        self.scroll_by(delta, cx);
    }

    fn measure(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let measured = self.measured.clone();
        let entity = cx.entity_id();
        canvas(
            move |bounds, window, _| {
                let now = Measured {
                    origin: point(f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
                    size: size(f32::from(bounds.size.width), f32::from(bounds.size.height)),
                    device: window.scale_factor(),
                };
                if measured.replace(now) != now {
                    window.on_next_frame(move |_, cx| cx.notify(entity));
                }
            },
            |_, (), _, _| {},
        )
        .absolute()
        .size_full()
    }

    pub(super) fn stage(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> ObservedElement<Stateful<Div>> {
        let sheets: Vec<AnyElement> = self
            .doc()
            .map(|doc| doc.shown().map(|page| sheet(t, doc, page)).collect())
            .unwrap_or_default();
        div()
            .id("pdf-stage")
            .test_support()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(t.colors.background)
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
                view.wheel(event, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, window, cx| {
                    view.press(event, window, cx);
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| view.drag(event, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, _| view.release()),
            )
            .child(self.measure(cx))
            .children(sheets)
    }
}
