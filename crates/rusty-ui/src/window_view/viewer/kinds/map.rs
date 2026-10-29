use rusty_automotive::sniff_map;

use super::{Load, Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::map::{MapBody, MapFile};

const LINKER_MAP: u8 = 70;

pub(super) struct MapKind;

fn is_map(sniff: &Sniff<'_>) -> bool {
    named(sniff.name, &["map"]) && !sniff.looks_binary() && sniff_map(sniff.head).is_some()
}

impl ViewerKind for MapKind {
    fn id(&self) -> &'static str {
        "linker-map"
    }

    fn name(&self) -> &'static str {
        "Linker map"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if is_map(sniff) { LINKER_MAP } else { 0 }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        if !is_map(load.sniff) {
            return Err("Not a linker map.".to_owned());
        }
        Ok(Box::new(MapFile {
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
            .downcast::<MapFile>()
            .map_err(|_| "The linker map view got data it cannot show.".to_owned())?;
        Ok(Box::new(MapBody::new(&file, &attach.host, cx)))
    }
}
