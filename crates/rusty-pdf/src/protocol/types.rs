use std::io;

use super::{MAX_PIXELS, MAX_SIDE, MAX_TEXT_CHARS, PageLinks};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Open {
        path: String,
        password: Option<String>,
    },
    PageCount,
    Render(RenderRequest),
    Text {
        page: u32,
    },
    Links {
        page: u32,
    },
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderRequest {
    pub page: u32,
    pub width: u32,
    pub rotation: Rotation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    None,
    Quarter,
    Half,
    ThreeQuarters,
}

impl Rotation {
    pub fn is_sideways(self) -> bool {
        matches!(self, Self::Quarter | Self::ThreeQuarters)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Response {
    Opened { pages: u32 },
    PageCount(u32),
    Page(Bitmap),
    Text(PageText),
    Links(PageLinks),
    Closed,
    Failed(PdfError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Bitmap {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, CodecError> {
        let pixels = check_size(width, height)?;
        if rgba.len() as u64 != pixels * 4 {
            return Err(CodecError::Malformed(
                "bitmap length does not match its size",
            ));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }
}

pub(crate) fn check_size(width: u32, height: u32) -> Result<u64, CodecError> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 {
        return Err(CodecError::Malformed("bitmap side is zero"));
    }
    if width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS {
        return Err(CodecError::TooLarge {
            what: "bitmap",
            len: pixels,
            max: MAX_PIXELS,
        });
    }
    Ok(pixels)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharBox {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl CharBox {
    pub(crate) fn is_finite(&self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .iter()
            .all(|value| value.is_finite())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextChar {
    pub ch: char,
    pub bounds: CharBox,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageText {
    width: f32,
    height: f32,
    chars: Vec<TextChar>,
}

impl PageText {
    pub fn new(width: f32, height: f32, chars: Vec<TextChar>) -> Result<Self, CodecError> {
        if !(width.is_finite() && height.is_finite()) {
            return Err(CodecError::Malformed("page size is not finite"));
        }
        if chars.len() > MAX_TEXT_CHARS {
            return Err(CodecError::TooLarge {
                what: "text",
                len: chars.len() as u64,
                max: MAX_TEXT_CHARS as u64,
            });
        }
        if !chars.iter().all(|text| text.bounds.is_finite()) {
            return Err(CodecError::Malformed("character box is not finite"));
        }
        Ok(Self {
            width,
            height,
            chars,
        })
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn chars(&self) -> &[TextChar] {
        &self.chars
    }

    pub fn text(&self) -> String {
        self.chars.iter().map(|text| text.ch).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PdfError {
    #[error("the PDF needs a password")]
    NeedsPassword,
    #[error("the password is wrong")]
    WrongPassword,
    #[error("the PDF is damaged")]
    Corrupt,
    #[error("the PDF or page is too large")]
    TooLarge,
    #[error("the PDF took too long")]
    Timeout,
    #[error("the PDF helper stopped")]
    HelperDied,
    #[error("{0}")]
    NoEngine(String),
    #[error("no PDF is open")]
    NotOpen,
    #[error("page {page} does not exist; the PDF has {count} pages")]
    NoSuchPage { page: u32, count: u32 },
    #[error("cannot read the PDF: {0}")]
    Unreadable(String),
    #[error("the PDF helper cannot start: {0}")]
    NoHelper(String),
    #[error("the PDF helper broke the protocol: {0}")]
    Protocol(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("frame ends early")]
    Truncated,
    #[error("{what} is {len}, over the limit of {max}")]
    TooLarge {
        what: &'static str,
        len: u64,
        max: u64,
    },
    #[error("malformed frame: {0}")]
    Malformed(&'static str),
    #[error("pipe failed: {0}")]
    Io(#[from] io::Error),
}

pub fn check_render_size(
    request: &RenderRequest,
    page_width: f32,
    page_height: f32,
) -> Result<(u32, u32), PdfError> {
    let (across, down) = if request.rotation.is_sideways() {
        (page_height, page_width)
    } else {
        (page_width, page_height)
    };
    if !(across.is_finite() && down.is_finite() && across > 0.0 && down > 0.0) {
        return Err(PdfError::Corrupt);
    }
    if request.width == 0 {
        return Err(PdfError::Protocol("render width is zero".into()));
    }
    let height = (f64::from(request.width) * f64::from(down) / f64::from(across))
        .round()
        .max(1.0);
    if height > f64::from(MAX_SIDE) {
        return Err(PdfError::TooLarge);
    }
    let height = height as u32;
    check_size(request.width, height).map_err(|_| PdfError::TooLarge)?;
    Ok((request.width, height))
}
