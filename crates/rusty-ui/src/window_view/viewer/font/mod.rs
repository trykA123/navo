mod body;
mod cmap;
mod draw;
mod face;
mod names;
mod sfnt;
mod woff;

use std::borrow::Cow;
use std::collections::HashMap;

use gpui_kit::{FocusHandle, Global, ScrollHandle, point};
use rusty_diag::components::{Held, MemoryUse, Tally};

pub(super) use body::FontBody;
#[cfg(test)]
pub(super) use draw::{SAMPLE, SIZES};
use face::Preview;
pub use face::{FontFace, font_format};

use crate::prelude::*;

const LINE_STEP: f32 = 40.;
const NO_PREVIEW: &str = "Preview not available for this font.";

static FONT_MEMORY: Tally = Tally::new("viewer fonts");

#[derive(Default)]
struct Registered(HashMap<u64, Held>);

impl Global for Registered {}

pub struct FontView {
    title: SharedString,
    face: FontFace,
    family: Option<SharedString>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    note: Option<SharedString>,
}

fn register(preview: Preview, cx: &mut App) -> Result<bool, String> {
    if cx
        .default_global::<Registered>()
        .0
        .contains_key(&preview.hash)
    {
        return Ok(false);
    }
    let bytes = preview.bytes.len() as u64;
    cx.text_system()
        .add_fonts(vec![Cow::Owned(preview.bytes)])
        .map_err(|error| format!("Cannot render this font: {error:#}"))?;
    let held = FONT_MEMORY.hold(MemoryUse { bytes, items: 1 });
    cx.default_global::<Registered>()
        .0
        .insert(preview.hash, held);
    Ok(true)
}

fn preview_family(face: &mut FontFace, cx: &mut App) -> Result<SharedString, String> {
    let preview = face.preview.take().ok_or(NO_PREVIEW)?;
    let family = preview.family.clone().into();
    register(preview, cx)?;
    Ok(family)
}

impl FontView {
    pub fn new(title: impl Into<SharedString>, mut face: FontFace, cx: &mut Context<Self>) -> Self {
        let (family, note) = match preview_family(&mut face, cx) {
            Ok(family) => (Some(family), None),
            Err(note) => (None, Some(note.into())),
        };
        Self {
            title: title.into(),
            face,
            family,
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
