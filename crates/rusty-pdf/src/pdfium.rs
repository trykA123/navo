mod links;
mod outline;

use std::path::{Path, PathBuf};

use pdfium_render::prelude::{
    PdfDocument, PdfPage, PdfPageRenderRotation, PdfRenderConfig, Pdfium, PdfiumError,
    PdfiumInternalError,
};

use crate::protocol::{
    Bitmap, CharBox, MAX_TEXT_CHARS, Outline, PageLinks, PageText, PdfError, RenderRequest,
    Rotation, TextChar, check_render_size,
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
        let (width, height) = (page.width().value, page.height().value);
        let turns = page.rotation().map_or(0, quarter_turns);
        let upright = Upright::new(turns, width, height);
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
                let bounds = upright.map(
                    (rect.left().value, rect.bottom().value),
                    (rect.right().value, rect.top().value),
                );
                bounds.is_finite().then_some(TextChar { ch, bounds })
            })
            .collect();
        PageText::new(width, height, chars).map_err(|_| PdfError::Corrupt)
    }

    fn links(&mut self, page: u32) -> Result<PageLinks, PdfError> {
        links::extract(self, self.page(page)?)
    }

    fn outline(&mut self) -> Result<Outline, PdfError> {
        outline::extract(self, self.document.as_ref().ok_or(PdfError::NotOpen)?)
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

fn quarter_turns(rotation: PdfPageRenderRotation) -> u8 {
    match rotation {
        PdfPageRenderRotation::None => 0,
        PdfPageRenderRotation::Degrees90 => 1,
        PdfPageRenderRotation::Degrees180 => 2,
        PdfPageRenderRotation::Degrees270 => 3,
    }
}

struct Upright {
    turns: u8,
    across: f32,
    tall: f32,
}

impl Upright {
    fn new(turns: u8, width: f32, height: f32) -> Self {
        let (across, tall) = if turns % 2 == 1 {
            (height, width)
        } else {
            (width, height)
        };
        Self {
            turns: turns % 4,
            across,
            tall,
        }
    }

    fn point(&self, (x, y): (f32, f32)) -> (f32, f32) {
        match self.turns {
            1 => (y, x),
            2 => (self.across - x, y),
            3 => (self.tall - y, self.across - x),
            _ => (x, self.tall - y),
        }
    }

    fn map(&self, corner: (f32, f32), other: (f32, f32)) -> CharBox {
        let (a, b) = (self.point(corner), self.point(other));
        CharBox {
            left: a.0.min(b.0),
            top: a.1.min(b.1),
            right: a.0.max(b.0),
            bottom: a.1.max(b.1),
        }
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

#[cfg(test)]
mod tests {
    use super::links::{destination_top, link_bounds, uri_target};
    use super::*;
    use crate::protocol::{LinkTarget, MAX_URI_BYTES};
    use pdfium_render::prelude::{PdfDestinationViewSettings as View, PdfPoints, PdfRect};

    fn mapped(turns: u8, width: f32, height: f32) -> CharBox {
        Upright::new(turns, width, height).map((10., 150.), (20., 160.))
    }

    #[test]
    fn char_boxes_follow_the_page_rotation_into_the_drawn_page() {
        let boxed = |left, top, right, bottom| CharBox {
            left,
            top,
            right,
            bottom,
        };
        assert_eq!(mapped(0, 100., 200.), boxed(10., 40., 20., 50.));
        assert_eq!(mapped(1, 200., 100.), boxed(150., 10., 160., 20.));
        assert_eq!(mapped(2, 100., 200.), boxed(80., 150., 90., 160.));
        assert_eq!(mapped(3, 200., 100.), boxed(40., 80., 50., 90.));
    }

    #[test]
    fn link_boxes_share_the_text_transform_and_drop_non_finite_bounds() {
        let rect = PdfRect::new_from_values(150., 10., 160., 20.);
        for turns in 0..4 {
            let upright = Upright::new(turns, 100., 200.);
            assert_eq!(
                link_bounds(&upright, rect),
                Some(upright.map((10., 150.), (20., 160.)))
            );
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for rect in [
                PdfRect::new_from_values(value, 10., 160., 20.),
                PdfRect::new_from_values(150., value, 160., 20.),
                PdfRect::new_from_values(150., 10., value, 20.),
                PdfRect::new_from_values(150., 10., 160., value),
            ] {
                assert_eq!(link_bounds(&Upright::new(0, 100., 200.), rect), None);
            }
        }
    }

    #[test]
    fn destination_tops_follow_the_destination_page_rotation() {
        let xyz = View::SpecificCoordinatesAndZoom(
            Some(PdfPoints::new(10.)),
            Some(PdfPoints::new(160.)),
            None,
        );
        for (turns, expected) in [(0, 40.), (1, 10.), (2, 160.), (3, 190.)] {
            assert_eq!(
                destination_top(&Upright::new(turns, 100., 200.), xyz),
                Some(expected)
            );
        }
    }

    #[test]
    fn destination_tops_keep_missing_coordinates_absent() {
        let upright = Upright::new(0, 100., 200.);
        for view in [
            View::Unknown,
            View::FitPageToWindow,
            View::FitBoundsToWindow,
            View::SpecificCoordinatesAndZoom(None, None, None),
            View::FitPageHorizontallyToWindow(None),
            View::FitBoundsHorizontallyToWindow(None),
            View::FitPageVerticallyToWindow(Some(PdfPoints::new(10.))),
            View::FitBoundsVerticallyToWindow(None),
        ] {
            assert_eq!(destination_top(&upright, view), None);
        }
        for view in [
            View::FitPageHorizontallyToWindow(Some(PdfPoints::new(160.))),
            View::FitBoundsHorizontallyToWindow(Some(PdfPoints::new(160.))),
            View::FitPageToRectangle(PdfRect::new_from_values(150., 10., 160., 20.)),
        ] {
            assert_eq!(destination_top(&upright, view), Some(40.));
        }
        assert_eq!(
            destination_top(
                &upright,
                View::FitPageHorizontallyToWindow(Some(PdfPoints::new(f32::NAN)))
            ),
            None
        );
        assert_eq!(
            destination_top(
                &Upright::new(1, 100., 200.),
                View::FitPageHorizontallyToWindow(Some(PdfPoints::new(160.)))
            ),
            None
        );
    }

    #[test]
    fn oversized_uris_become_unsupported_by_byte_length() {
        let uri = "é".repeat(MAX_URI_BYTES / 2);
        assert_eq!(uri_target(uri.clone()), LinkTarget::Uri(uri.clone()));
        assert_eq!(uri_target(format!("{uri}é")), LinkTarget::Unsupported);
    }
}
