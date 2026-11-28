use super::{Load, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::paged::PagedBody;
use crate::window_view::viewer::{HexFileStamp, Paged, PagedKind};

const BINARY: u8 = 60;
const TEXT: u8 = 10;

struct HexLoaded {
    paged: Paged,
    stamp: Option<HexFileStamp>,
}

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

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        let paged = load.hooks.paged_with(load.location, load.encoding)?;
        let stamp = (load.hooks.hex_stamp)(load.location).ok().flatten();
        Ok(Box::new(HexLoaded { paged, stamp }))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let loaded = attach
            .loaded
            .downcast::<HexLoaded>()
            .map_err(|_| "The hex view got data it cannot show.".to_owned())?;
        Ok(Box::new(PagedBody::new_hex(
            attach.host,
            loaded.paged,
            loaded.stamp,
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
