use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_core::paging::{LineIndex, TextLayout};

use super::IndexResult;

use super::INDEX_PROGRESS_EVERY;

const CHUNK: usize = 1024 * 1024;

pub fn index_lines(
    path: &Path,
    layout: TextLayout,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&LineIndex),
) -> IndexResult {
    let file = File::open(path).map_err(|error| format!("Cannot open it: {error}"))?;
    index_reader(file, layout, cancel, progress)
}

pub(super) fn index_reader(
    mut file: impl Read,
    layout: TextLayout,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&LineIndex),
) -> IndexResult {
    let mut index = LineIndex::with_layout(layout);
    let unit = layout.unit();
    let mut buf = vec![0u8; CHUNK];
    let mut held = 0;
    let mut reported = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let count = match file.read(&mut buf[held..]) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("Cannot read it: {error}")),
        };
        let total = held + count;
        let whole = total - total % unit;
        index.feed(&buf[..whole]);
        buf.copy_within(whole..total, 0);
        held = total - whole;
        if index.scanned() - reported >= INDEX_PROGRESS_EVERY {
            reported = index.scanned();
            progress(&index);
        }
    }
    index.feed(&buf[..held]);
    index.finish();
    progress(&index);
    Ok(Some(index))
}
