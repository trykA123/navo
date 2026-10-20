use std::sync::{Arc, Mutex};

use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use rusty_pdf::protocol::{
    Bitmap, OutlineItem, PageLinks, PageText, PdfError, RenderRequest, Rotation,
};

use super::source::PdfPages;

pub(super) type Shared = Arc<Mutex<Box<dyn PdfPages>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Job {
    Page { page: u32, width: u32 },
    Thumb { page: u32, width: u32 },
    Text { page: u32 },
    Links { page: u32 },
    Outline,
}

impl Job {
    pub(super) fn page(self) -> u32 {
        match self {
            Self::Page { page, .. }
            | Self::Thumb { page, .. }
            | Self::Text { page }
            | Self::Links { page } => page,
            Self::Outline => 0,
        }
    }
}

pub(super) struct Picture {
    pub width: u32,
    pub height: u32,
    pub image: Arc<RenderImage>,
}

pub(super) enum Done {
    Drawn(Job, Picture),
    Read(u32, Arc<PageText>),
    Linked(u32, Arc<PageLinks>),
    Outlined(Vec<OutlineItem>),
    Failed(Job, PdfError),
}

fn to_image(bitmap: Bitmap) -> Result<Picture, PdfError> {
    let (width, height) = (bitmap.width(), bitmap.height());
    let mut pixels = bitmap.into_rgba();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let rgba = RgbaImage::from_raw(width, height, pixels).ok_or(PdfError::Corrupt)?;
    Ok(Picture {
        width,
        height,
        image: Arc::new(RenderImage::new(vec![Frame::new(rgba)])),
    })
}

fn draw(pages: &mut dyn PdfPages, page: u32, width: u32) -> Result<Picture, PdfError> {
    let request = RenderRequest {
        page,
        width,
        rotation: Rotation::None,
    };
    pages.render(request).and_then(to_image)
}

pub(super) fn run(shared: &Shared, job: Job) -> Done {
    let Ok(mut pages) = shared.lock() else {
        return Done::Failed(job, PdfError::HelperDied);
    };
    let pages = pages.as_mut();
    let drawn = match job {
        Job::Page { page, width } | Job::Thumb { page, width } => draw(pages, page, width),
        Job::Links { page } => {
            return match pages.links(page) {
                Ok(links) => Done::Linked(page, Arc::new(links)),
                Err(error) => Done::Failed(job, error),
            };
        }
        Job::Outline => {
            return match pages.outline() {
                Ok(outline) => Done::Outlined(outline.items().to_vec()),
                Err(error) => Done::Failed(job, error),
            };
        }
        Job::Text { page } => {
            return match pages.text(page) {
                Ok(text) => Done::Read(page, Arc::new(text)),
                Err(error) => Done::Failed(job, error),
            };
        }
    };
    match drawn {
        Ok(picture) => Done::Drawn(job, picture),
        Err(error) => Done::Failed(job, error),
    }
}
