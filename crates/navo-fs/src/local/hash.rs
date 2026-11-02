use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use crate::CancelToken;

const CHUNK: usize = 256 * 1024;

pub(super) fn blake3_of(path: &Path, cancel: &CancelToken) -> io::Result<Option<[u8; 32]>> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; CHUNK];
    loop {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let read = match file.read(&mut buffer) {
            Ok(0) => return Ok(Some(*hasher.finalize().as_bytes())),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(buffer.get(..read).unwrap_or_default());
    }
}
