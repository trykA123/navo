use rusty_automotive::{EXTENSIONS, sniff};

use super::{Load, Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::memory::{MemoryBody, MemoryFile};

const MEMORY_IMAGE: u8 = 70;

pub(super) struct MemoryKind;

impl ViewerKind for MemoryKind {
    fn id(&self) -> &'static str {
        "memory"
    }

    fn name(&self) -> &'static str {
        "Memory image"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if named(sniff.name, &EXTENSIONS) && self::sniff(sniff.head).is_some() {
            MEMORY_IMAGE
        } else {
            0
        }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        let format = sniff(load.sniff.head).ok_or_else(|| "Not a memory image.".to_owned())?;
        Ok(Box::new(MemoryFile {
            len: load.sniff.len,
            format,
        }))
    }

    fn attach(
        &self,
        attach: Attach,
        _: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let file = attach
            .loaded
            .downcast::<MemoryFile>()
            .map_err(|_| "The memory image view got data it cannot show.".to_owned())?;
        Ok(Box::new(MemoryBody::new(&file, attach.host, cx)))
    }
}
