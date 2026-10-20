use std::sync::Arc;

use rusty_core::Location;
use rusty_pdf::Document;
use rusty_pdf::protocol::{Bitmap, Outline, PageLinks, PageText, PdfError, RenderRequest};

pub trait PdfPages: Send {
    fn page_count(&self) -> u32;
    fn render(&mut self, request: RenderRequest) -> Result<Bitmap, PdfError>;
    fn text(&mut self, page: u32) -> Result<PageText, PdfError>;
    fn links(&mut self, page: u32) -> Result<PageLinks, PdfError>;
    fn outline(&mut self) -> Result<Outline, PdfError>;
}

impl PdfPages for Document {
    fn page_count(&self) -> u32 {
        self.pages()
    }

    fn render(&mut self, request: RenderRequest) -> Result<Bitmap, PdfError> {
        Document::render(self, request)
    }

    fn text(&mut self, page: u32) -> Result<PageText, PdfError> {
        Document::text(self, page)
    }

    fn links(&mut self, page: u32) -> Result<PageLinks, PdfError> {
        Document::links(self, page)
    }

    fn outline(&mut self) -> Result<Outline, PdfError> {
        Document::outline(self)
    }
}

pub type OpenPdf =
    Arc<dyn Fn(&Location, Option<&str>) -> Result<Box<dyn PdfPages>, PdfError> + Send + Sync>;

pub(super) const NO_ENGINE: &str = "PDF view is not available in this build.";

pub(in crate::window_view::viewer) fn no_pdf() -> OpenPdf {
    Arc::new(|_, _| Err(PdfError::NoEngine(NO_ENGINE.to_owned())))
}
