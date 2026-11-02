use std::{
    io::BufRead,
    sync::atomic::{AtomicBool, Ordering},
};

use super::lines::{Lines, Step};
use super::tasking::Tasking;
use super::{MapError, MapModel};

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
    let mut decoder = Decoder::default();
    loop {
        line.clear();
        match lines.next(&mut line)? {
            Step::End => break,
            Step::Skip(bytes) => {
                offset += bytes;
                parser.flush_row();
                parser.clear_file();
            }
            Step::Line { bytes, too_long } => {
                offset += bytes;
                recognized |= decoder.take(&mut parser, &line, too_long);
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
    parser.flush_row();
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

#[derive(Default)]
struct Decoder {
    evidence: super::parts::Evidence,
    lossy: bool,
}

impl Decoder {
    fn take(&mut self, parser: &mut Tasking, raw: &[u8], too_long: bool) -> bool {
        parser.model.lines += 1;
        if too_long {
            parser.drop_row("the line exceeds 4096 bytes");
            return self.evidence.take("");
        }
        let text = String::from_utf8_lossy(raw);
        if !self.lossy && matches!(text, std::borrow::Cow::Owned(_)) {
            self.lossy = true;
            parser.model.malformed("lossy UTF-8 decoding was required");
        }
        let recognized = self.evidence.take(&text);
        parser.take(&text);
        recognized
    }
}
