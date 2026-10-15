use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_core::paging::LineIndex;

use super::IndexResult;

use super::INDEX_PROGRESS_EVERY;

const CHUNK: usize = 1024 * 1024;

pub fn index_lines(
    path: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&LineIndex),
) -> IndexResult {
    let mut file = File::open(path).map_err(|error| format!("Cannot open it: {error}"))?;
    let mut index = LineIndex::new();
    let mut buf = vec![0u8; CHUNK];
    let mut reported = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let count = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("Cannot read it: {error}")),
        };
        index.feed(&buf[..count]);
        if index.scanned() - reported >= INDEX_PROGRESS_EVERY {
            reported = index.scanned();
            progress(&index);
        }
    }
    index.finish();
    progress(&index);
    Ok(Some(index))
}
