//! Opens a PDF from a `File` the helper adopted from Navo, so pdfium never sees a path.
//! The document owns the file; Prepare and Close drop the document and with it the handle.

use std::fs::File;

use super::{MAX_FILE_BYTES, PdfiumEngine, open_error, page_total};
use crate::protocol::PdfError;
use crate::serve::clip;

/// Loads `file` into `engine` and returns the page count.
/// Fails with `TooLarge` above `MAX_FILE_BYTES`, `Unreadable` when its size cannot be read, or the pdfium error.
pub(super) fn open_file(
    engine: &mut PdfiumEngine,
    file: File,
    password: Option<&str>,
) -> Result<u32, PdfError> {
    engine.document = None;
    let size = file
        .metadata()
        .map_err(|error| PdfError::Unreadable(clip(error.to_string())))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(PdfError::TooLarge);
    }
    let document = engine
        .pdfium
        .load_pdf_from_reader(file, password)
        .map_err(|error| open_error(&error, password.is_some()))?;
    let pages = page_total(&document);
    engine.document = Some(document);
    Ok(pages)
}
