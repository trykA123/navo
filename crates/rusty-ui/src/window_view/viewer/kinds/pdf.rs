use rusty_pdf::protocol::PdfError;

use super::{Load, Loaded, Slot, Sniff, ViewerKind};
use crate::prelude::*;
use crate::window_view::viewer::body::{Attach, ViewerBody};
use crate::window_view::viewer::pdf::{PdfBody, PdfPages, PdfStart, PdfView};

pub(in crate::window_view::viewer) const PDF_ID: &str = "pdf";
const PDF: u8 = 95;
const MAGIC: &[u8] = b"%PDF-";
const MAGIC_WITHIN: usize = 1024;

pub(in crate::window_view::viewer) fn is_pdf(head: &[u8]) -> bool {
    let head = &head[..head.len().min(MAGIC_WITHIN)];
    head.windows(MAGIC.len()).any(|window| window == MAGIC)
}

struct Opened {
    pages: Result<Box<dyn PdfPages>, PdfError>,
    len: u64,
}

pub(super) struct Pdf;

impl ViewerKind for Pdf {
    fn id(&self) -> &'static str {
        PDF_ID
    }

    fn name(&self) -> &'static str {
        "PDF"
    }

    fn slot(&self) -> Slot {
        Slot::Rich
    }

    fn score(&self, sniff: &Sniff<'_>) -> u8 {
        if is_pdf(sniff.head) { PDF } else { 0 }
    }

    fn load(&self, load: Load<'_>) -> Result<Loaded, String> {
        load.ensure_open()?;
        let pages = (load.hooks.pdf)(load.location, None);
        Ok(Box::new(Opened {
            pages,
            len: load.sniff.len,
        }))
    }

    fn attach(
        &self,
        attach: Attach,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Box<dyn ViewerBody>, String> {
        let opened = attach
            .loaded
            .downcast::<Opened>()
            .map_err(|_| "The PDF view got data it cannot show.".to_owned())?;
        let start = PdfStart {
            title: attach.host.title,
            len: opened.len,
            location: attach.host.location,
            open: attach.host.hooks.pdf,
            opened: opened.pages,
        };
        let view = cx.new(|cx| PdfView::new(start, window, cx));
        Ok(Box::new(PdfBody(view)))
    }
}
