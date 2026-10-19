use std::path::{Path, PathBuf};

use pdfium_render::prelude::{
    PdfDocument, PdfPage, PdfPageRenderRotation, PdfRenderConfig, Pdfium, PdfiumError,
    PdfiumInternalError,
};

use crate::protocol::{
    Bitmap, CharBox, MAX_TEXT_CHARS, PageText, PdfError, RenderRequest, Rotation, TextChar,
    check_render_size,
};
use crate::serve::{Engine, clip};

pub const MAX_FILE_BYTES: u64 = 1 << 30;

pub fn library_beside(dir: &Path) -> PathBuf {
    Pdfium::pdfium_platform_library_name_at_path(dir)
}

pub struct PdfiumEngine {
    pdfium: &'static Pdfium,
    document: Option<PdfDocument<'static>>,
}

impl PdfiumEngine {
    pub fn load(library: &Path) -> Result<Self, String> {
        let bindings = Pdfium::bind_to_library(library)
            .map_err(|error| format!("cannot load {}: {error:?}", library.display()))?;
        Ok(Self::from_pdfium(Pdfium::new(bindings)))
    }

    pub fn load_system() -> Result<Self, String> {
        let bindings = Pdfium::bind_to_system_library()
            .map_err(|error| format!("no system pdfium: {error:?}"))?;
        Ok(Self::from_pdfium(Pdfium::new(bindings)))
    }

    fn from_pdfium(pdfium: Pdfium) -> Self {
        Self {
            pdfium: Box::leak(Box::new(pdfium)),
            document: None,
        }
    }

    fn page(&self, index: u32) -> Result<PdfPage<'static>, PdfError> {
        let document = self.document.as_ref().ok_or(PdfError::NotOpen)?;
        let count = page_total(document);
        if index >= count {
            return Err(PdfError::NoSuchPage { page: index, count });
        }
        let index = i32::try_from(index).map_err(|_| PdfError::TooLarge)?;
        document.pages().get(index).map_err(|_| PdfError::Corrupt)
    }
}

impl Engine for PdfiumEngine {
    fn open(&mut self, path: &str, password: Option<&str>) -> Result<u32, PdfError> {
        self.document = None;
        let size = std::fs::metadata(path)
            .map_err(|error| PdfError::Unreadable(clip(error.to_string())))?
            .len();
        if size > MAX_FILE_BYTES {
            return Err(PdfError::TooLarge);
        }
        let document = self
            .pdfium
            .load_pdf_from_file(path, password)
            .map_err(|error| open_error(error, password.is_some()))?;
        let pages = page_total(&document);
        self.document = Some(document);
        Ok(pages)
    }

    fn page_count(&self) -> Result<u32, PdfError> {
        self.document
            .as_ref()
            .map(page_total)
            .ok_or(PdfError::NotOpen)
    }

    fn render(&mut self, request: &RenderRequest) -> Result<Bitmap, PdfError> {
        let page = self.page(request.page)?;
        let (width, _) = check_render_size(request, page.width().value, page.height().value)?;
        let across = if request.rotation.is_sideways() {
            page.height().value
        } else {
            page.width().value
        };
        let config = PdfRenderConfig::new()
            .scale_page_by_factor(width as f32 / across)
            .rotate(render_rotation(request.rotation), true);
        let rendered = page
            .render_with_config(&config)
            .map_err(|_| PdfError::Corrupt)?;
        let (width, height) = (rendered.width(), rendered.height());
        let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
            return Err(PdfError::Corrupt);
        };
        Bitmap::new(width, height, rendered.as_rgba_bytes()).map_err(|_| PdfError::TooLarge)
    }

    fn text(&mut self, page: u32) -> Result<PageText, PdfError> {
        let page = self.page(page)?;
        let height = page.height().value;
        let text = page.text().map_err(|_| PdfError::Corrupt)?;
        if text.chars().len() > MAX_TEXT_CHARS {
            return Err(PdfError::TooLarge);
        }
        let chars = text
            .chars()
            .iter()
            .filter_map(|item| {
                let ch = item.unicode_char()?;
                let rect = item.loose_bounds().ok()?;
                let bounds = CharBox {
                    left: rect.left().value,
                    top: height - rect.top().value,
                    right: rect.right().value,
                    bottom: height - rect.bottom().value,
                };
                bounds.is_finite().then_some(TextChar { ch, bounds })
            })
            .collect();
        PageText::new(page.width().value, height, chars).map_err(|_| PdfError::Corrupt)
    }

    fn close(&mut self) {
        self.document = None;
    }
}

fn page_total(document: &PdfDocument<'_>) -> u32 {
    u32::try_from(document.pages().len()).unwrap_or(0)
}

fn render_rotation(rotation: Rotation) -> PdfPageRenderRotation {
    match rotation {
        Rotation::None => PdfPageRenderRotation::None,
        Rotation::Quarter => PdfPageRenderRotation::Degrees90,
        Rotation::Half => PdfPageRenderRotation::Degrees180,
        Rotation::ThreeQuarters => PdfPageRenderRotation::Degrees270,
    }
}

fn open_error(error: PdfiumError, has_password: bool) -> PdfError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            if has_password {
                PdfError::WrongPassword
            } else {
                PdfError::NeedsPassword
            }
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError)
        | PdfiumError::IoError(_) => PdfError::Unreadable("the file cannot be read".into()),
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            PdfError::Unreadable("its security handler is not supported".into())
        }
        _ => PdfError::Corrupt,
    }
}
