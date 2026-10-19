use std::{
    io::BufRead,
    sync::atomic::{AtomicBool, Ordering},
};

use super::lines::{Lines, Step};
use super::tasking::Tasking;
use super::{MapError, MapModel, sniff_map};

pub fn index_map(
    read: impl BufRead,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<MapModel, MapError> {
    check_cancel(cancel)?;
    let mut parser = Tasking::new();
    let mut lines = Lines::new(read);
    let mut line = Vec::with_capacity(super::MAX_MAP_LINE);
    let mut offset = 0;
    let mut next_check = 64 * 1024;
    let mut recognized = false;
    loop {
        line.clear();
        match lines.next(&mut line)? {
            Step::End => break,
            Step::Skip(bytes) => offset += bytes,
            Step::Line { bytes, too_long } => {
                offset += bytes;
                parser.model.lines += 1;
                if too_long {
                    parser.model.malformed("the line exceeds 4096 bytes");
                } else {
                    recognized |= !recognized && sniff_map(&line).is_some();
                    parser.take(&line);
                }
            }
        }
        if offset >= next_check {
            check_cancel(cancel)?;
            progress(offset);
            next_check = offset.saturating_add(64 * 1024);
        }
    }
    check_cancel(cancel)?;
    if !recognized {
        return Err(MapError::UnsupportedFormat);
    }
    super::finish::finish(&mut parser.model);
    check_cancel(cancel)?;
    progress(offset);
    check_cancel(cancel)?;
    Ok(parser.model)
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), MapError> {
    if cancel.load(Ordering::Relaxed) {
        Err(MapError::Cancelled)
    } else {
        Ok(())
    }
}
