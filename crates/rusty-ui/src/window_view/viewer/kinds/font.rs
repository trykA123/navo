use super::{Load, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::font::{FontBody, FontFace, FontView, font_format};

const FONT: u8 = 70;
const LOOKS_LIKE_TEXT: u8 = 5;
const FONT_LIMIT: u64 = 32 * 1024 * 1024;

pub(super) struct Font;

impl ViewerKind for Font {
    fn id(&self) -> &'static str {
        "font"
    }

    fn name(&self) -> &'static str {
        "Font"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        match font_format(sniff.head) {
            None => 0,
            Some(_) if sniff.looks_binary() => FONT,
            Some(_) => LOOKS_LIKE_TEXT,
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        let bytes = (load.hooks.bytes)(load.location, FONT_LIMIT)?;
        Ok(Box::new(FontFace::parse(bytes)?))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let face = attach
            .loaded
            .downcast::<FontFace>()
            .map_err(|_| "The font view got data it cannot show.".to_owned())?;
        let title = attach.host.title;
        let view = cx.new(|cx| FontView::new(title, *face, cx));
        Ok(Box::new(FontBody(view)))
    }
}
