//! Drawing the image body: the view picker with its keys, the pair, the overlay wipe and the
//! difference mask. Every colour comes from a theme token; the wipe is dragged or keyed.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, Bounds, Context, Corners, CursorStyle, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ObjectFit, Pixels, RenderImage, Role, Stateful, StyledImage,
    canvas, fill, img, point, px, size,
};
use navo_l10n::tr;

use super::{Pictures, Pointer, Slot};
use crate::compare_diff::image_body::ImageMode;
use crate::prelude::*;
use crate::primitives::{Button, ButtonVariant, Kbd};
use crate::tokens::Theme;
use crate::window_view::ScreenOwner;

/// The view each bare digit opens; the key shows beside its button.
const VIEWS: [(char, ImageMode); 3] = [
    ('1', ImageMode::SideBySide),
    ('2', ImageMode::Overlay),
    ('3', ImageMode::Mask),
];

/// The image body: the picker over the open view of the pair.
pub(in crate::window_view::compare_screen) fn body(
    t: &Theme,
    body: &Pictures,
    cx: &mut Context<ScreenOwner>,
) -> AnyElement {
    let view = match body.mode() {
        ImageMode::Overlay => wipe(t, body, cx),
        ImageMode::Mask => mask(t, body),
        ImageMode::SideBySide => pair(t, body),
    };
    div()
        .id("compare-image")
        .test_support()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .bg(t.colors.background)
        .children(picker(t, body, cx))
        .child(view)
        .into_any_element()
}

/// The view picker, and why the pair shows less; `None` in the Simple look.
fn picker(t: &Theme, body: &Pictures, cx: &mut Context<ScreenOwner>) -> Option<AnyElement> {
    if !body.parts() {
        return None;
    }
    Some(
        div()
            .id("compare-image-picker")
            .test_support()
            .flex()
            .flex_none()
            .items_center()
            .gap(t.spacing.md)
            .px(t.spacing.md)
            .h(t.spacing.tab_height)
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.rail)
            .children(VIEWS.into_iter().map(|view| view_button(t, body, view, cx)))
            .child(div().flex_1())
            .children(body.notice().map(|id| {
                div()
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .id("compare-image-notice")
                    .test_support()
                    .child(tr(id))
            }))
            .into_any_element(),
    )
}

/// One picker button: its label, its key, dimmed when this pair cannot show that view.
fn view_button(
    t: &Theme,
    body: &Pictures,
    (key, mode): (char, ImageMode),
    cx: &mut Context<ScreenOwner>,
) -> AnyElement {
    let open = body.mode();
    let label = tr(mode.label());
    let tip = format!("{label} ({key})");
    let weak = cx.weak_entity();
    div()
        .id(("compare-image-view", mode.index()))
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .child(
            Button::new(("compare-image-pick", mode.index()), label)
                .variant(if open == mode {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                })
                .disabled(!body.shows(mode))
                .tooltip(tip)
                .on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |view, cx| view.image_mode(mode, cx));
                }),
        )
        .children(Kbd::new(
            ("compare-image-key", mode.index()),
            &key.to_string(),
        ))
        .into_any_element()
}

/// The two sides, each in its own half of the stage.
fn pair(t: &Theme, body: &Pictures) -> AnyElement {
    div()
        .id("compare-image-pair")
        .test_support()
        .flex()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(side(t, body, 0))
        .child(div().flex_none().w(px(1.)).bg(t.colors.line))
        .child(side(t, body, 1))
        .into_any_element()
}

/// One side: its caption, then the picture or the line that says why there is none.
fn side(t: &Theme, body: &Pictures, at: usize) -> AnyElement {
    let (note, image) = match &body.slots[at] {
        Slot::Idle => (None, None),
        Slot::Loading => (Some(tr("compare-image-loading")), None),
        Slot::Failed => (body.notice().map(tr), None),
        Slot::Ready(image, width, height) => (
            Some(format!("{} × {}", width, height)),
            Some(Arc::clone(image)),
        ),
    };
    div()
        .id(("compare-image-side", at))
        .test_support()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(t.spacing.sm)
                .px(t.spacing.md)
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(side_name(at))
                .children(note),
        )
        .when_some(image, |pane, image| {
            pane.child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .child(img(image).size_full().object_fit(ObjectFit::Contain)),
            )
        })
        .into_any_element()
}

/// The name of one side.
fn side_name(at: usize) -> String {
    tr(if at == 0 {
        "compare-image-left"
    } else {
        "compare-image-right"
    })
}

/// The difference mask on its own: bright where the pictures differ.
fn mask(t: &Theme, body: &Pictures) -> AnyElement {
    div()
        .id("compare-image-mask")
        .test_support()
        .flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .p(t.spacing.md)
        .when_some(body.mask_image.clone(), |stage, image| {
            stage.child(img(image).size_full().object_fit(ObjectFit::Contain))
        })
        .into_any_element()
}

/// The overlay: both pictures in one box, cut where the wipe sits. It is a slider for the
/// keyboard and for assistive tools, and the pointer drags it.
fn wipe(t: &Theme, body: &Pictures, cx: &mut Context<ScreenOwner>) -> AnyElement {
    div()
        .id("compare-image-wipe")
        .test_support()
        .role(Role::Slider)
        .aria_label(tr("compare-image-wipe-label"))
        .aria_value(format!("{:.0} %", body.split() * 100.0))
        .aria_keyshortcuts(tr("compare-image-wipe-keys"))
        .tab_index(0)
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(stage(t, body, cx))
        .child(
            div()
                .flex_none()
                .px(t.spacing.md)
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(tr("compare-image-wipe-hint")),
        )
        .into_any_element()
}

/// The wipe's drawing surface: it records where the pictures sit and answers the pointer.
fn stage(t: &Theme, body: &Pictures, cx: &mut Context<ScreenOwner>) -> AnyElement {
    let (left, right) = (body.picture(0), body.picture(1));
    let ((width, height), split) = (body.side_size().unwrap_or((0., 0.)), body.split());
    let (record, line) = (Rc::clone(&body.stage), t.colors.line);
    div()
        .id("compare-image-stage")
        .relative()
        .flex_1()
        .min_h_0()
        .cursor(CursorStyle::ResizeLeftRight)
        .child(
            canvas(
                move |bounds, _, _| {
                    let (placed, at) = place(bounds, width, height, split);
                    *record.borrow_mut() = Some(placed);
                    (placed, at)
                },
                move |_, (placed, at), window, _| {
                    let [near, far] = halves(placed, at);
                    cut(window, placed, near, left.as_ref());
                    cut(window, placed, far, right.as_ref());
                    let mark =
                        Bounds::new(point(at, placed.origin.y), size(px(2.), placed.size.height));
                    window.paint_quad(fill(mark, line));
                },
            )
            .absolute()
            .size_full(),
        )
        .map(|stage| pointer_events(stage, cx))
        .into_any_element()
}

/// Wires the stage's pointer events to the wipe: press, drag, release inside or outside.
fn pointer_events(stage: Stateful<Div>, cx: &mut Context<ScreenOwner>) -> Stateful<Div> {
    stage
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, event: &MouseDownEvent, _, cx| {
                view.image_pointer(Pointer::Down, event.position.x, cx);
            }),
        )
        .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
            let held = event.pressed_button == Some(MouseButton::Left);
            view.image_pointer(Pointer::Move(held), event.position.x, cx);
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|view, event: &MouseUpEvent, _, cx| {
                view.image_pointer(Pointer::Up, event.position.x, cx);
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|view, event: &MouseUpEvent, _, cx| {
                view.image_pointer(Pointer::Up, event.position.x, cx);
            }),
        )
}

/// The wipe's box for both sides and the cut across it, from the last frame's stage.
fn place(bounds: Bounds<Pixels>, width: f32, height: f32, split: f32) -> (Bounds<Pixels>, Pixels) {
    let (box_w, box_h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    if width < 1. || height < 1. {
        return (bounds, bounds.origin.x + bounds.size.width * split);
    }
    let scale = (box_w / width.max(1.)).min(box_h / height.max(1.)).min(1.);
    let (wide, high) = (width * scale, height * scale);
    let placed = Bounds::new(
        point(
            bounds.origin.x + px((box_w - wide) / 2.),
            bounds.origin.y + px((box_h - high) / 2.),
        ),
        size(px(wide), px(high)),
    );
    (placed, placed.origin.x + placed.size.width * split)
}

/// The two halves of the placed pictures either side of the cut `at`: left, then right.
fn halves(placed: Bounds<Pixels>, at: Pixels) -> [Bounds<Pixels>; 2] {
    let height = placed.size.height;
    [
        Bounds::new(placed.origin, size(at - placed.origin.x, height)),
        Bounds::new(
            point(at, placed.origin.y),
            size(placed.origin.x + placed.size.width - at, height),
        ),
    ]
}

/// Draws `image` over `placed`, showing only the part inside `clip`.
fn cut(
    window: &mut Window,
    placed: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    image: Option<&Arc<RenderImage>>,
) {
    let Some(image) = image else { return };
    let _ = window.paint_image(clip, placed, Corners::default(), image.clone(), 0, false);
}

/// The file diff root's image keys: `1`..`3` open a view, the arrows and Home/End move the wipe.
pub(in crate::window_view::compare_screen) fn image_keys(
    root: Stateful<Div>,
    cx: &mut Context<ScreenOwner>,
) -> Stateful<Div> {
    root.on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let key = event.keystroke.key.clone();
        if view.image_key(&key) {
            cx.notify();
        }
    }))
}
