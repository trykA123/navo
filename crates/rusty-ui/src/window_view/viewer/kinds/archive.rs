use rusty_fs::archive::{ArchiveListing, detect};

use super::{Load, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::archive::{ArchiveBody, ArchiveSource, ArchiveView};
use crate::window_view::viewer::body::{Attach, ViewerBody};

const ARCHIVE: u8 = 70;
const LOOKS_LIKE_TEXT: u8 = 5;

pub(super) struct Archive;

impl ViewerKind for Archive {
    fn id(&self) -> &'static str {
        "archive"
    }

    fn name(&self) -> &'static str {
        "Archive"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        match detect(sniff.head, sniff.name) {
            None => 0,
            Some(_) if sniff.looks_binary() => ARCHIVE,
            Some(_) => LOOKS_LIKE_TEXT,
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        Ok(Box::new((load.hooks.archive)(load.location, load.cancel)?))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let listing = attach
            .loaded
            .downcast::<ArchiveListing>()
            .map_err(|_| "The archive view got data it cannot show.".to_owned())?;
        let source = ArchiveSource {
            location: attach.host.location,
            hooks: attach.host.hooks,
        };
        let title = attach.host.title;
        let view = cx.new(|cx| ArchiveView::new(title, source, *listing, cx));
        Ok(Box::new(ArchiveBody(view)))
    }
}
