use std::thread;
use std::time::Duration;

use crate::protocol::{
    Bitmap, CharBox, LinkTarget, PageLink, PageLinks, PageText, PdfError, RenderRequest, TextChar,
    check_render_size,
};
use crate::serve::Engine;

pub const PAGES: u32 = 12;
pub const HANG_PAGE: u32 = 7;
pub const CRASH_PAGE: u32 = 9;
pub const PASSWORD: &str = "secret";

#[derive(Default)]
pub struct FakeEngine {
    is_open: bool,
}

impl Engine for FakeEngine {
    fn open(&mut self, path: &str, password: Option<&str>) -> Result<u32, PdfError> {
        if path.ends_with("broken.pdf") {
            return Err(PdfError::Corrupt);
        }
        if path.ends_with("locked.pdf") {
            match password {
                None => return Err(PdfError::NeedsPassword),
                Some(PASSWORD) => {}
                Some(_) => return Err(PdfError::WrongPassword),
            }
        }
        self.is_open = true;
        Ok(PAGES)
    }

    fn page_count(&self) -> Result<u32, PdfError> {
        self.check_open()?;
        Ok(PAGES)
    }

    fn render(&mut self, request: &RenderRequest) -> Result<Bitmap, PdfError> {
        self.check_page(request.page)?;
        match request.page {
            HANG_PAGE => thread::sleep(Duration::from_secs(3600)),
            CRASH_PAGE => std::process::exit(3),
            _ => {}
        }
        let (width, height) = check_render_size(request, 100.0, 200.0)?;
        let shade = request.page as u8;
        let rgba = [shade, 0, 0, 255].repeat(width as usize * height as usize);
        Bitmap::new(width, height, rgba).map_err(|_| PdfError::TooLarge)
    }

    fn text(&mut self, page: u32) -> Result<PageText, PdfError> {
        self.check_page(page)?;
        let chars = format!("page {page}")
            .chars()
            .enumerate()
            .map(|(index, ch)| TextChar {
                ch,
                bounds: CharBox {
                    left: index as f32 * 10.0,
                    top: 0.0,
                    right: index as f32 * 10.0 + 9.0,
                    bottom: 12.0,
                },
            })
            .collect();
        PageText::new(100.0, 200.0, chars).map_err(|_| PdfError::TooLarge)
    }

    fn links(&mut self, page: u32) -> Result<PageLinks, PdfError> {
        self.check_page(page)?;
        let targets = [
            LinkTarget::Page {
                page: (page + 1) % PAGES,
                top: Some(20.0),
            },
            LinkTarget::Uri(format!("https://example.org/page/{page}")),
            LinkTarget::Unsupported,
        ];
        let links = targets
            .into_iter()
            .enumerate()
            .map(|(index, target)| PageLink {
                bounds: CharBox {
                    left: 10.0,
                    top: 20.0 + index as f32 * 20.0,
                    right: 40.0,
                    bottom: 32.0 + index as f32 * 20.0,
                },
                target,
            })
            .collect();
        PageLinks::new(100.0, 200.0, links).map_err(|_| PdfError::TooLarge)
    }

    fn close(&mut self) {
        self.is_open = false;
    }
}

impl FakeEngine {
    fn check_open(&self) -> Result<(), PdfError> {
        if self.is_open {
            Ok(())
        } else {
            Err(PdfError::NotOpen)
        }
    }

    fn check_page(&self, page: u32) -> Result<(), PdfError> {
        self.check_open()?;
        if page < PAGES {
            Ok(())
        } else {
            Err(PdfError::NoSuchPage { page, count: PAGES })
        }
    }
}
