mod decode;
mod draw;
mod exif;
mod view;
mod zoom;

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{AnyView, FocusHandle, Focusable, point};

pub use decode::{IMAGE_LIMIT, Picture, decode_image};
pub use exif::ExifEntry;

use super::body::{BodyKeys, ViewerBody};
use crate::prelude::*;
use zoom::{STEP, Viewport, Zoom};

const PAN_STEP: f32 = 64.;

pub struct ImageView {
    title: SharedString,
    picture: Picture,
    pub(super) zoom: Zoom,
    viewport: Rc<Cell<Viewport>>,
    pub(super) info: bool,
    note: Option<SharedString>,
    drag: Option<Point<Pixels>>,
    focus: FocusHandle,
}

impl ImageView {
    pub fn new(title: impl Into<SharedString>, picture: Picture, cx: &mut Context<Self>) -> Self {
        cx.on_release(|view: &mut Self, cx| cx.drop_image(view.picture.pixels.clone(), None))
            .detach();
        let zoom = Zoom::new(picture.width, picture.height, picture.vector);
        Self {
            title: title.into(),
            picture,
            zoom,
            viewport: Rc::default(),
            info: false,
            note: None,
            drag: None,
            focus: cx.focus_handle(),
        }
    }

    pub fn show_info(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.info = shown;
        cx.notify();
    }

    fn viewport(&self) -> Viewport {
        self.viewport.get()
    }

    pub(super) fn zoom_label(&self) -> String {
        self.zoom.label(&self.viewport())
    }

    fn fit(&mut self, cx: &mut Context<Self>) {
        self.zoom.fit();
        cx.notify();
    }

    fn actual_size(&mut self, cx: &mut Context<Self>) {
        let viewport = self.viewport();
        self.zoom.zoom_to(1., Zoom::centre_of(&viewport), &viewport);
        cx.notify();
    }

    fn zoom_step(&mut self, inward: bool, anchor: Option<Point<f32>>, cx: &mut Context<Self>) {
        let viewport = self.viewport();
        let factor = if inward { STEP } else { 1. / STEP };
        let anchor = anchor.unwrap_or_else(|| Zoom::centre_of(&viewport));
        self.zoom.zoom_by(factor, anchor, &viewport);
        cx.notify();
    }

    fn pan_by(&mut self, delta: Point<f32>, cx: &mut Context<Self>) {
        let viewport = self.viewport();
        self.zoom.pan_by(delta, &viewport);
        cx.notify();
    }

    fn pan_step(&mut self, columns: f32, rows: f32, cx: &mut Context<Self>) {
        self.pan_by(point(columns * PAN_STEP, rows * PAN_STEP), cx);
    }

    fn toggle_info(&mut self, cx: &mut Context<Self>) {
        self.info = !self.info;
        cx.notify();
    }

    fn set_note(&mut self, note: SharedString, cx: &mut Context<Self>) {
        self.note = Some(note);
        cx.notify();
    }
}

#[cfg(test)]
impl ImageView {
    pub(super) fn pan(&self) -> Point<f32> {
        self.zoom.pan
    }
}

impl Focusable for ImageView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

pub(super) struct ImageBody(pub Entity<ImageView>);

#[cfg(test)]
impl ImageBody {
    pub(super) fn of(body: &dyn ViewerBody) -> Option<&Self> {
        body.as_any().downcast_ref::<Self>()
    }
}

impl ViewerBody for ImageBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_handle(cx)
    }

    fn keys(&self, _: &App) -> BodyKeys {
        BodyKeys::Normal(vec![
            ("0", "Fit"),
            ("1", "100 %"),
            ("+/-", "Zoom"),
            ("I", "Info"),
            ("Space", "Next image"),
        ])
    }

    fn find_again(&self, _: bool, _: &mut Window, _: &mut App) {}

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
pub(super) mod fixtures;
#[cfg(test)]
mod tests;
