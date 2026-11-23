//! Tests for `serve_with`: a helper with an adopt hook opens only handles, one without opens only paths.

use std::fs::File;
use std::io::{self, Cursor};

use super::*;
use crate::protocol::{OpenSource, read_response, write_request};

const TINY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tiny.pdf");

/// An engine that records how it was asked to open a document.
#[derive(Default)]
struct Recorder {
    file_len: Option<u64>,
    path: Option<String>,
}

impl Engine for Recorder {
    fn open(&mut self, path: &str, _password: Option<&str>) -> Result<u32, PdfError> {
        self.path = Some(path.to_owned());
        Ok(1)
    }

    fn open_file(&mut self, file: File, _password: Option<&str>) -> Result<u32, PdfError> {
        self.file_len = Some(file.metadata().map_err(|_| PdfError::Corrupt)?.len());
        Ok(1)
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

/// A fake adopt hook: value 7 is the tiny fixture, every other value is refused.
fn adopt_tiny(value: u64) -> io::Result<File> {
    if value == 7 {
        File::open(TINY)
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "not shared"))
    }
}

/// Sends one Open request through `serve_with` and returns the single reply.
fn open_with(engine: &mut dyn Engine, source: OpenSource, adopt: Option<AdoptFile>) -> Response {
    let mut wire = Vec::new();
    let open = Request::Open {
        source,
        password: None,
    };
    write_request(&mut wire, &open).unwrap();
    let mut output = Vec::new();
    serve_with(engine, &mut Cursor::new(wire), &mut output, adopt).unwrap();
    read_response(&mut Cursor::new(output)).unwrap().unwrap()
}

#[test]
fn a_shared_handle_opens_the_adopted_file() {
    let mut engine = Recorder::default();
    let reply = open_with(&mut engine, OpenSource::Handle(7), Some(adopt_tiny));
    assert_eq!(reply, Response::Opened { pages: 1 });
    assert_eq!(
        engine.file_len,
        Some(std::fs::metadata(TINY).unwrap().len())
    );
}

#[test]
fn a_handle_the_hook_refuses_fails_before_the_engine() {
    let mut engine = Recorder::default();
    let reply = open_with(&mut engine, OpenSource::Handle(8), Some(adopt_tiny));
    assert!(matches!(reply, Response::Failed(PdfError::Unreadable(_))));
    assert_eq!(engine.file_len, None);
}

#[test]
fn a_path_is_refused_by_a_helper_that_adopts_handles() {
    let mut engine = Recorder::default();
    let reply = open_with(&mut engine, OpenSource::Path(TINY.into()), Some(adopt_tiny));
    let refusal = "this helper opens only a handle from Navo".to_owned();
    assert_eq!(reply, Response::Failed(PdfError::Unreadable(refusal)));
    assert_eq!(engine.path, None);
}

#[test]
fn a_handle_is_refused_by_a_helper_without_an_adopt_hook() {
    let mut engine = Recorder::default();
    let reply = open_with(&mut engine, OpenSource::Handle(7), None);
    let refusal = "this helper cannot open a handle".to_owned();
    assert_eq!(reply, Response::Failed(PdfError::Unreadable(refusal)));
    assert_eq!(engine.file_len, None);
}

#[test]
fn engines_without_handle_support_refuse_a_file() {
    let file = File::open(TINY).unwrap();
    let error = Unavailable::new("no pdfium")
        .open_file(file, None)
        .unwrap_err();
    assert_eq!(error, PdfError::NoEngine("no pdfium".into()));
}
