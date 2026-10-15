mod body;
mod draw;
mod face;
mod woff;

use std::borrow::Cow;
use std::collections::HashSet;

use gpui_kit::{FocusHandle, Global, ScrollHandle, point};

pub(super) use body::FontBody;
#[cfg(test)]
pub(super) use draw::{SAMPLE, SIZES};
pub use face::{FontFace, font_format};

use crate::prelude::*;

const LINE_STEP: f32 = 40.;

#[derive(Default)]
struct Registered(HashSet<u64>);

impl Global for Registered {}

pub struct FontView {
    title: SharedString,
    face: FontFace,
    focus: FocusHandle,
    scroll: ScrollHandle,
    note: Option<SharedString>,
}

fn register(face: &FontFace, cx: &mut App) -> Result<(), String> {
    if cx.default_global::<Registered>().0.contains(&face.hash) {
        return Ok(());
    }
    let bytes = Cow::Owned(face.sfnt.as_ref().clone());
    cx.text_system()
        .add_fonts(vec![bytes])
        .map_err(|error| format!("Cannot render this font: {error:#}"))?;
    cx.default_global::<Registered>().0.insert(face.hash);
    Ok(())
}

impl FontView {
    pub fn new(title: impl Into<SharedString>, face: FontFace, cx: &mut Context<Self>) -> Self {
        let note = register(&face, cx).err().map(SharedString::from);
        Self {
            title: title.into(),
            face,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            note,
        }
    }

    fn scroll_by(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        let offset = self.scroll.offset();
        let lowest = -self.scroll.max_offset().y;
        let y = (offset.y - delta).clamp(lowest, px(0.));
        self.scroll.set_offset(point(offset.x, y));
        cx.notify();
    }

    fn page(&self) -> Pixels {
        let height = self.scroll.bounds().size.height;
        if height > px(LINE_STEP) {
            height - px(LINE_STEP)
        } else {
            px(LINE_STEP)
        }
    }

    pub(super) fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }
}

impl gpui_kit::Focusable for FontView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests;
