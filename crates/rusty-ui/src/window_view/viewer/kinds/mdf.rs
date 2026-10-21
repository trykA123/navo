use rusty_automotive::sniff_mdf;
use rusty_l10n::t;

use super::{Load, Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::mdf::{MdfBody, MdfFile};

const MEMORY_DEFINITION: u8 = 70;

pub(super) struct MdfKind;

fn is_mdf(sniff: &Sniff<'_>) -> bool {
    named(sniff.name, &["mdf"]) && !sniff.looks_binary() && sniff_mdf(sniff.head)
}

impl ViewerKind for MdfKind {
    fn id(&self) -> &'static str {
        "memory-definition"
    }

    fn name(&self) -> &'static str {
        "MDF"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if is_mdf(sniff) { MEMORY_DEFINITION } else { 0 }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        if !is_mdf(load.sniff) {
            return Err(t!("viewer-mdf-not-mdf"));
        }
        Ok(Box::new(MdfFile {
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
            .downcast::<MdfFile>()
            .map_err(|_| t!("viewer-mdf-bad-data"))?;
        Ok(Box::new(MdfBody::new(*file, attach.host, cx)))
    }
}
