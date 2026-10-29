use rusty_automotive::sniff_elf;

use super::{Load, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::elf::{ElfBody, ElfFile};

const ELF_IMAGE: u8 = 70;

pub(super) struct ElfKind;

impl ViewerKind for ElfKind {
    fn id(&self) -> &'static str {
        "elf"
    }

    fn name(&self) -> &'static str {
        "ELF"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if sniff_elf(sniff.head) { ELF_IMAGE } else { 0 }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        if !sniff_elf(load.sniff.head) {
            return Err("Not an ELF file.".to_owned());
        }
        Ok(Box::new(ElfFile {
            len: load.sniff.len,
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
            .downcast::<ElfFile>()
            .map_err(|_| "The ELF view got data it cannot show.".to_owned())?;
        Ok(Box::new(ElfBody::new(&file, &attach.host, cx)))
    }
}
