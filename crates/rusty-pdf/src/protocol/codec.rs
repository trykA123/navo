mod links;
mod outline;

use super::types::{CharBox, check_size};
use super::{
    Bitmap, CodecError, MAX_MESSAGE_BYTES, MAX_PASSWORD_BYTES, MAX_PATH_BYTES, MAX_TEXT_CHARS,
    PageText, PdfError, RenderRequest, Request, Response, Rotation, TextChar,
};

const CHAR_BYTES: usize = 20;

pub fn encode_request(request: &Request) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::new();
    match request {
        Request::Open { path, password } => {
            out.push(1);
            put_str(&mut out, "path", path, MAX_PATH_BYTES)?;
            put_optional(&mut out, password.as_deref())?;
        }
        Request::PageCount => out.push(2),
        Request::Render(render) => {
            out.push(3);
            put_u32(&mut out, render.page);
            put_u32(&mut out, render.width);
            out.push(rotation_tag(render.rotation));
        }
        Request::Text { page } => {
            out.push(4);
            put_u32(&mut out, *page);
        }
        Request::Close => out.push(5),
        Request::Links { page } => {
            out.push(6);
            put_u32(&mut out, *page);
        }
        Request::Outline => out.push(7),
    }
    Ok(out)
}

pub fn decode_request(bytes: &[u8]) -> Result<Request, CodecError> {
    let mut input = Input(bytes);
    let request = match input.u8()? {
        1 => Request::Open {
            path: input.string("path", MAX_PATH_BYTES)?,
            password: input.optional()?,
        },
        2 => Request::PageCount,
        3 => Request::Render(RenderRequest {
            page: input.u32()?,
            width: input.u32()?,
            rotation: rotation(input.u8()?)?,
        }),
        4 => Request::Text { page: input.u32()? },
        5 => Request::Close,
        6 => Request::Links { page: input.u32()? },
        7 => Request::Outline,
        _ => return Err(CodecError::Malformed("unknown request")),
    };
    input.finish(request)
}

pub fn encode_response(response: &Response) -> Result<Vec<u8>, CodecError> {
    let mut out = Vec::new();
    match response {
        Response::Opened { pages } => {
            out.push(1);
            put_u32(&mut out, *pages);
        }
        Response::PageCount(count) => {
            out.push(2);
            put_u32(&mut out, *count);
        }
        Response::Page(bitmap) => put_bitmap(&mut out, bitmap),
        Response::Text(text) => put_text(&mut out, text),
        Response::Links(links) => links::put_links(&mut out, links)?,
        Response::Outline(outline) => outline::put_outline(&mut out, outline)?,
        Response::Closed => out.push(5),
        Response::Failed(error) => {
            out.push(6);
            put_error(&mut out, error)?;
        }
    }
    Ok(out)
}

pub fn decode_response(bytes: &[u8]) -> Result<Response, CodecError> {
    let mut input = Input(bytes);
    let response = match input.u8()? {
        1 => Response::Opened {
            pages: input.u32()?,
        },
        2 => Response::PageCount(input.u32()?),
        3 => Response::Page(bitmap(&mut input)?),
        4 => Response::Text(text(&mut input)?),
        5 => Response::Closed,
        6 => Response::Failed(error(&mut input)?),
        7 => Response::Links(links::links(&mut input)?),
        8 => Response::Outline(outline::outline(&mut input)?),
        _ => return Err(CodecError::Malformed("unknown response")),
    };
    input.finish(response)
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_str(
    out: &mut Vec<u8>,
    what: &'static str,
    value: &str,
    max: usize,
) -> Result<(), CodecError> {
    if value.len() > max {
        return Err(CodecError::TooLarge {
            what,
            len: value.len() as u64,
            max: max as u64,
        });
    }
    put_u32(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn put_optional(out: &mut Vec<u8>, password: Option<&str>) -> Result<(), CodecError> {
    match password {
        None => out.push(0),
        Some(password) => {
            out.push(1);
            put_str(out, "password", password, MAX_PASSWORD_BYTES)?;
        }
    }
    Ok(())
}

fn put_bitmap(out: &mut Vec<u8>, bitmap: &Bitmap) {
    out.reserve(9 + bitmap.rgba().len());
    out.push(3);
    put_u32(out, bitmap.width());
    put_u32(out, bitmap.height());
    out.extend_from_slice(bitmap.rgba());
}

fn put_text(out: &mut Vec<u8>, text: &PageText) {
    out.reserve(13 + text.chars().len() * CHAR_BYTES);
    out.push(4);
    put_f32(out, text.width());
    put_f32(out, text.height());
    put_u32(out, text.chars().len() as u32);
    for item in text.chars() {
        put_u32(out, u32::from(item.ch));
        let bounds = item.bounds;
        for value in [bounds.left, bounds.top, bounds.right, bounds.bottom] {
            put_f32(out, value);
        }
    }
}

fn put_error(out: &mut Vec<u8>, error: &PdfError) -> Result<(), CodecError> {
    let (tag, message) = match error {
        PdfError::NeedsPassword => (1, None),
        PdfError::WrongPassword => (2, None),
        PdfError::Corrupt => (3, None),
        PdfError::TooLarge => (4, None),
        PdfError::Timeout => (5, None),
        PdfError::HelperDied => (6, None),
        PdfError::NoEngine(message) => (7, Some(message)),
        PdfError::NotOpen => (8, None),
        PdfError::NoSuchPage { page, count } => {
            out.push(9);
            put_u32(out, *page);
            put_u32(out, *count);
            return Ok(());
        }
        PdfError::Unreadable(message) => (10, Some(message)),
        PdfError::NoHelper(message) => (11, Some(message)),
        PdfError::Protocol(message) => (12, Some(message)),
    };
    out.push(tag);
    match message {
        Some(message) => put_str(out, "message", message, MAX_MESSAGE_BYTES),
        None => Ok(()),
    }
}

fn rotation_tag(rotation: Rotation) -> u8 {
    match rotation {
        Rotation::None => 0,
        Rotation::Quarter => 1,
        Rotation::Half => 2,
        Rotation::ThreeQuarters => 3,
    }
}

fn rotation(tag: u8) -> Result<Rotation, CodecError> {
    match tag {
        0 => Ok(Rotation::None),
        1 => Ok(Rotation::Quarter),
        2 => Ok(Rotation::Half),
        3 => Ok(Rotation::ThreeQuarters),
        _ => Err(CodecError::Malformed("unknown rotation")),
    }
}

fn bitmap(input: &mut Input<'_>) -> Result<Bitmap, CodecError> {
    let width = input.u32()?;
    let height = input.u32()?;
    let pixels = check_size(width, height)?;
    let rgba = input.take(pixels as usize * 4)?.to_vec();
    Bitmap::new(width, height, rgba)
}

fn text(input: &mut Input<'_>) -> Result<PageText, CodecError> {
    let width = input.f32()?;
    let height = input.f32()?;
    let count = input.u32()? as usize;
    if count > MAX_TEXT_CHARS {
        return Err(CodecError::TooLarge {
            what: "text",
            len: count as u64,
            max: MAX_TEXT_CHARS as u64,
        });
    }
    if input.0.len() < count * CHAR_BYTES {
        return Err(CodecError::Truncated);
    }
    let chars = (0..count)
        .map(|_| text_char(input))
        .collect::<Result<Vec<_>, _>>()?;
    PageText::new(width, height, chars)
}

fn text_char(input: &mut Input<'_>) -> Result<TextChar, CodecError> {
    let ch = char::from_u32(input.u32()?).ok_or(CodecError::Malformed("invalid character"))?;
    let bounds = CharBox {
        left: input.f32()?,
        top: input.f32()?,
        right: input.f32()?,
        bottom: input.f32()?,
    };
    Ok(TextChar { ch, bounds })
}

fn error(input: &mut Input<'_>) -> Result<PdfError, CodecError> {
    Ok(match input.u8()? {
        1 => PdfError::NeedsPassword,
        2 => PdfError::WrongPassword,
        3 => PdfError::Corrupt,
        4 => PdfError::TooLarge,
        5 => PdfError::Timeout,
        6 => PdfError::HelperDied,
        7 => PdfError::NoEngine(input.message()?),
        8 => PdfError::NotOpen,
        9 => PdfError::NoSuchPage {
            page: input.u32()?,
            count: input.u32()?,
        },
        10 => PdfError::Unreadable(input.message()?),
        11 => PdfError::NoHelper(input.message()?),
        12 => PdfError::Protocol(input.message()?),
        _ => return Err(CodecError::Malformed("unknown error")),
    })
}

struct Input<'a>(&'a [u8]);

impl<'a> Input<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], CodecError> {
        let (head, rest) = self.0.split_at_checked(len).ok_or(CodecError::Truncated)?;
        self.0 = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        let mut bytes = [0; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> Result<f32, CodecError> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn string(&mut self, what: &'static str, max: usize) -> Result<String, CodecError> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(CodecError::TooLarge {
                what,
                len: len as u64,
                max: max as u64,
            });
        }
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| CodecError::Malformed("text is not UTF-8"))
    }

    fn message(&mut self) -> Result<String, CodecError> {
        self.string("message", MAX_MESSAGE_BYTES)
    }

    fn optional(&mut self) -> Result<Option<String>, CodecError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.string("password", MAX_PASSWORD_BYTES)?)),
            _ => Err(CodecError::Malformed("unknown option tag")),
        }
    }

    fn finish<T>(self, value: T) -> Result<T, CodecError> {
        if self.0.is_empty() {
            Ok(value)
        } else {
            Err(CodecError::Malformed("trailing bytes"))
        }
    }
}
