use std::path::PathBuf;

use super::{ClipboardError, FileClipboard};

pub fn write_files(_files: &FileClipboard) -> Result<(), ClipboardError> {
    Err(ClipboardError::Unsupported)
}

pub fn read_files() -> Result<Option<FileClipboard>, ClipboardError> {
    Err(ClipboardError::Unsupported)
}

pub fn clear_if_same(_files: &FileClipboard) -> Result<bool, ClipboardError> {
    Err(ClipboardError::Unsupported)
}

pub(super) fn decode_ansi(bytes: &[u8]) -> Result<PathBuf, ClipboardError> {
    if !bytes.is_ascii() {
        return Err(ClipboardError::Unsupported);
    }
    String::from_utf8(bytes.to_vec())
        .map(PathBuf::from)
        .map_err(|_| ClipboardError::Malformed)
}
