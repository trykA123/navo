use rusty_automotive::{Evidence, TRACE_EXTENSIONS, sniff_blf, sniff_trace};

use super::{Load, Loaded, Slot, Sniff, ViewerKind, named};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::trace::{TraceBody, TraceFile};

const CAN_TRACE: u8 = 70;

pub(super) struct TraceKind;

fn is_trace(sniff: &Sniff<'_>) -> bool {
    if sniff_blf(sniff.head) {
        return true;
    }
    if sniff.looks_binary() {
        return false;
    }
    match sniff_trace(sniff.head) {
        Some(Evidence::Header) => true,
        Some(Evidence::Frames) => named(sniff.name, &TRACE_EXTENSIONS),
        None => false,
    }
}

impl ViewerKind for TraceKind {
    fn id(&self) -> &'static str {
        "can-trace"
    }

    fn name(&self) -> &'static str {
        "CAN trace"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if is_trace(sniff) { CAN_TRACE } else { 0 }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        if !is_trace(load.sniff) {
            return Err("Not a CAN trace.".to_owned());
        }
        Ok(Box::new(TraceFile {
            len: load.sniff.len,
            head: load.sniff.head.to_vec(),
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
            .downcast::<TraceFile>()
            .map_err(|_| "The CAN trace view got data it cannot show.".to_owned())?;
        Ok(Box::new(TraceBody::new(&file, attach.host, cx)))
    }
}
