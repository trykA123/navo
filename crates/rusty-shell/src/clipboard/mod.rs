mod model;
#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

pub use model::{ClipboardError, Effect, FileClipboard, encode_drop_files};
pub use platform::{clear_if_same, has_files, read_files, write_files};

pub fn decode_drop_files(bytes: &[u8]) -> Result<Vec<std::path::PathBuf>, ClipboardError> {
    model::decode_drop_files(bytes, platform::decode_ansi)
}

#[cfg(test)]
mod tests;
