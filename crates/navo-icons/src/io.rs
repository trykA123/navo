//! Bounded file readers for untrusted icon inputs.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Why a bounded file read failed.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The file contained more bytes than the configured limit.
    #[error("file exceeds size limit")]
    TooLarge,
    /// An underlying filesystem or I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Reads at most `cap` bytes from `path`, failing closed if more data exists.
pub fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>, ReadError> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(cap.saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(ReadError::TooLarge);
    }
    Ok(bytes)
}
