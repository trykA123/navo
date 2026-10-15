use rusty_core::Location;

use super::{Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::paged::PagedBody;
use crate::window_view::viewer::{FileHooks, Paged, PagedKind};

const BINARY: u8 = 60;
const TEXT: u8 = 10;

pub(super) struct Hex;

impl ViewerKind for Hex {
    fn id(&self) -> &'static str {
        "hex"
    }

    fn name(&self) -> &'static str {
        "Hex"
    }

    fn slot(&self) -> Slot {
        Slot::Hex
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff.looks_binary() { BINARY } else { TEXT }
    }

    fn load(
        &self,
        hooks: &FileHooks,
        location: &Location,
        _: &Sniff<'_>,
    ) -> Result<Loaded, String> {
        Ok(Box::new((hooks.paged)(location)?))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let paged = attach
            .loaded
            .downcast::<Paged>()
            .map_err(|_| "The hex view got data it cannot show.".to_owned())?;
        Ok(Box::new(PagedBody::new(
            attach.host,
            *paged,
            PagedKind::Hex,
            cx,
        )))
    }

    fn reuse(&self, body: &dyn ViewerBody, _: &Sniff<'_>, cx: &mut App) -> bool {
        let Some(paged) = PagedBody::of(body) else {
            return false;
        };
        paged.show_as(PagedKind::Hex, cx);
        true
    }
}
