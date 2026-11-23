use std::fs::File;
use std::io::{self, Read, Write};

use crate::protocol::{
    Bitmap, CodecError, MAX_MESSAGE_BYTES, MAX_RESPONSE_FRAME, OpenSource, Outline, PageLinks,
    PageText, PdfError, RenderRequest, Request, Response, encode_response, read_request,
    write_frame, write_response,
};

pub trait Engine {
    fn prepare(&mut self) -> Result<(), PdfError> {
        self.close();
        Ok(())
    }

    fn open(&mut self, path: &str, password: Option<&str>) -> Result<u32, PdfError>;

    /// Opens the document from a file the helper adopted; returns the page count.
    /// Engines that cannot read a file answer `Unreadable`.
    fn open_file(&mut self, _file: File, _password: Option<&str>) -> Result<u32, PdfError> {
        Err(PdfError::Unreadable(
            "this engine cannot open a handle".into(),
        ))
    }

    fn page_count(&self) -> Result<u32, PdfError>;
    fn render(&mut self, request: &RenderRequest) -> Result<Bitmap, PdfError>;
    fn text(&mut self, page: u32) -> Result<PageText, PdfError>;
    fn links(&mut self, page: u32) -> Result<PageLinks, PdfError>;
    fn outline(&mut self) -> Result<Outline, PdfError>;
    fn close(&mut self);
}

pub struct Unavailable(String);

impl Unavailable {
    pub fn new(reason: impl Into<String>) -> Self {
        Self(clip(reason.into()))
    }
}

impl Engine for Unavailable {
    fn prepare(&mut self) -> Result<(), PdfError> {
        Err(PdfError::NoEngine(self.0.clone()))
    }

    fn open(&mut self, _path: &str, _password: Option<&str>) -> Result<u32, PdfError> {
        Err(PdfError::NoEngine(self.0.clone()))
    }

    fn open_file(&mut self, _file: File, _password: Option<&str>) -> Result<u32, PdfError> {
        Err(PdfError::NoEngine(self.0.clone()))
    }

    fn page_count(&self) -> Result<u32, PdfError> {
        Err(PdfError::NotOpen)
    }

    fn render(&mut self, _request: &RenderRequest) -> Result<Bitmap, PdfError> {
        Err(PdfError::NotOpen)
    }

    fn text(&mut self, _page: u32) -> Result<PageText, PdfError> {
        Err(PdfError::NotOpen)
    }

    fn links(&mut self, _page: u32) -> Result<PageLinks, PdfError> {
        Err(PdfError::NotOpen)
    }

    fn outline(&mut self) -> Result<Outline, PdfError> {
        Err(PdfError::NotOpen)
    }

    fn close(&mut self) {}
}

/// Turns a handle value the parent shared into this helper into an owned `File`.
pub type AdoptFile = fn(u64) -> io::Result<File>;

/// Answers requests from `input` on `output` until Close or end of input; opens only paths.
pub fn serve(
    engine: &mut dyn Engine,
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<(), CodecError> {
    serve_with(engine, input, output, None)
}

/// Like `serve`; with `adopt`, Open takes only a shared handle, without it only a path.
/// Fails when a request frame is malformed or the output cannot be written.
pub fn serve_with(
    engine: &mut dyn Engine,
    input: &mut impl Read,
    output: &mut impl Write,
    adopt: Option<AdoptFile>,
) -> Result<(), CodecError> {
    loop {
        let request = match read_request(input) {
            Ok(Some(request)) => request,
            Ok(None) => return Ok(()),
            Err(error) => {
                let refusal = Response::Failed(PdfError::Protocol(clip(error.to_string())));
                let _ = write_response(output, &refusal);
                return Err(error);
            }
        };
        let is_close = request == Request::Close;
        let payload = match encode_response(&answer(engine, request, adopt)) {
            Ok(payload) => payload,
            Err(_) => encode_response(&Response::Failed(PdfError::TooLarge))?,
        };
        write_frame(output, &payload, MAX_RESPONSE_FRAME)?;
        if is_close {
            return Ok(());
        }
    }
}

fn answer(engine: &mut dyn Engine, request: Request, adopt: Option<AdoptFile>) -> Response {
    let result = match request {
        Request::Open { source, password } => {
            open(engine, source, password.as_deref(), adopt).map(|pages| Response::Opened { pages })
        }
        Request::Prepare => engine.prepare().map(|()| Response::Closed),
        Request::PageCount => engine.page_count().map(Response::PageCount),
        Request::Render(render) => engine.render(&render).map(Response::Page),
        Request::Text { page } => engine.text(page).map(Response::Text),
        Request::Links { page } => engine.links(page).map(Response::Links),
        Request::Outline => engine.outline().map(Response::Outline),
        Request::Close => {
            engine.close();
            Ok(Response::Closed)
        }
    };
    result.unwrap_or_else(Response::Failed)
}

/// Opens the document from `source`; a shared handle is adopted first, before the engine or the
/// password is touched, so every later error drops the adopted `File`.
fn open(
    engine: &mut dyn Engine,
    source: OpenSource,
    password: Option<&str>,
    adopt: Option<AdoptFile>,
) -> Result<u32, PdfError> {
    match (source, adopt) {
        (OpenSource::Handle(value), Some(adopt)) => {
            let file =
                adopt(value).map_err(|error| PdfError::Unreadable(clip(error.to_string())))?;
            engine.open_file(file, password)
        }
        (OpenSource::Handle(_), None) => Err(PdfError::Unreadable(
            "this helper cannot open a handle".into(),
        )),
        (OpenSource::Path(_), Some(_)) => Err(PdfError::Unreadable(
            "this helper opens only a handle from Navo".into(),
        )),
        (OpenSource::Path(path), None) => engine.open(&path, password),
    }
}

pub fn clip(mut message: String) -> String {
    if message.len() > MAX_MESSAGE_BYTES {
        let mut end = MAX_MESSAGE_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
    message
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_handle;
