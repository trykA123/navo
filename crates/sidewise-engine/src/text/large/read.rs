use navo_core::paging::{ByteSource, TextLayout, read_full};

use crate::model::Reason;
use crate::text::scan::BLOCK;
use crate::text::side::{Line, LineEnding, LineSet};

use super::batch::{SideData, Stop};
use super::{LargeSide, PROVISIONAL_BYTES};

const FIRST_READ: usize = 64 * 1024;
const STOPPED: &str = "stopped";

pub(super) struct Lines {
    pub(super) lines: Vec<Line>,
    pub(super) starts: Vec<u64>,
    pub(super) bytes: Vec<u8>,
    pub(super) base: u64,
}

impl Lines {
    pub(super) fn set(&self, bom: bool) -> LineSet<'_> {
        LineSet {
            lines: &self.lines,
            starts: &self.starts,
            bom,
            bytes: None,
        }
    }
}

pub(super) fn head(side: &LargeSide, lines: u64, stop: Stop<'_>) -> Result<Lines, Reason> {
    let len = side.source.len();
    let until = Until {
        offset: 0,
        lines,
        end: len.min(PROVISIONAL_BYTES),
        len,
    };
    collect(side.source.as_ref(), side.layout, until, stop)
}

/// Byte offset where `line` starts, or the end of the side when it is past the last line.
pub(super) fn end_of(side: &SideData, line: u64, stop: Stop<'_>) -> Result<u64, Reason> {
    if line >= side.lines {
        Ok(side.len)
    } else {
        start_of(side, line, stop)
    }
}

pub(super) fn start_of(side: &SideData, line: u64, stop: Stop<'_>) -> Result<u64, Reason> {
    if line == 0 {
        return u64::try_from(side.layout.bom_len()).map_err(|_| Reason::ReadError);
    }
    let seek = Seek {
        source: side.source.as_ref(),
        layout: side.layout,
        stop,
    };
    side.index
        .line_start(&seek, line + 1)
        .map_err(|_| {
            if stop() {
                Reason::Stopped
            } else {
                Reason::ReadError
            }
        })?
        .ok_or(Reason::ChangedOnDisk)
}

struct Seek<'a> {
    source: &'a dyn ByteSource,
    layout: TextLayout,
    stop: Stop<'a>,
}

impl ByteSource for Seek<'_> {
    fn len(&self) -> u64 {
        self.source.len()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if (self.stop)() {
            return Err(STOPPED.to_owned());
        }
        self.source.read_at(offset, buf)
    }

    fn layout(&self) -> TextLayout {
        self.layout
    }
}

pub(super) struct Until {
    pub(super) offset: u64,
    pub(super) lines: u64,
    pub(super) end: u64,
    pub(super) len: u64,
}

pub(super) fn collect(
    source: &dyn ByteSource,
    layout: TextLayout,
    until: Until,
    stop: Stop<'_>,
) -> Result<Lines, Reason> {
    let offset = if until.offset == 0 {
        u64::try_from(layout.bom_len()).map_err(|_| Reason::ReadError)?
    } else {
        until.offset
    };
    let mut lines = Collector::new(layout, offset);
    let mut buffer = Vec::new();
    let mut chunk = FIRST_READ;
    let mut at = offset;
    // The estimate reserved these bytes; growing by doubling would not be counted.
    let span = usize::try_from(until.end.saturating_sub(offset)).map_err(|_| Reason::ReadError)?;
    lines.done.bytes.reserve_exact(span);
    while lines.count() < until.lines && at < until.end {
        if stop() {
            return Err(Reason::Stopped);
        }
        let want =
            usize::try_from((until.end - at).min(chunk as u64)).map_err(|_| Reason::ReadError)?;
        buffer.resize(want, 0);
        let read = read_full(source, at, &mut buffer).map_err(|_| Reason::ReadError)?;
        if read < want {
            return Err(Reason::ChangedOnDisk);
        }
        lines.done.bytes.extend_from_slice(&buffer[..read]);
        lines.feed(&buffer, at, until.lines);
        at += u64::try_from(read).map_err(|_| Reason::ReadError)?;
        chunk = (chunk * 2).min(BLOCK);
    }
    if lines.count() < until.lines && at == until.len {
        lines.finish(at);
    }
    Ok(lines.done)
}

struct Collector {
    layout: TextLayout,
    done: Lines,
    pending: Vec<u8>,
    bom: bool,
}

impl Collector {
    fn new(layout: TextLayout, offset: u64) -> Self {
        let bom = layout.text.bom && usize::try_from(offset).ok() == Some(layout.bom_len());
        Self {
            layout,
            done: Lines {
                lines: Vec::new(),
                starts: vec![offset],
                bytes: Vec::new(),
                base: offset,
            },
            pending: Vec::new(),
            bom,
        }
    }

    fn count(&self) -> u64 {
        u64::try_from(self.done.lines.len()).unwrap_or(u64::MAX)
    }

    fn feed(&mut self, chunk: &[u8], base: u64, limit: u64) {
        let unit = self.layout.unit();
        let mut from = 0;
        for newline in self.layout.newlines(chunk) {
            self.pending.extend_from_slice(&chunk[from..newline]);
            from = newline + unit;
            self.push(true, base + from as u64);
            if self.count() >= limit {
                return;
            }
        }
        self.pending.extend_from_slice(&chunk[from..]);
    }

    fn finish(&mut self, end: u64) {
        if !self.pending.is_empty() {
            self.push(false, end);
        }
    }

    fn push(&mut self, terminated: bool, next: u64) {
        let unit = self.layout.unit();
        let cr = terminated
            && self.pending.len() >= unit
            && self
                .layout
                .is_cr(&self.pending[self.pending.len() - unit..]);
        let ending = match (terminated, cr) {
            (true, true) => LineEnding::CrLf,
            (true, false) => LineEnding::Lf,
            (false, _) => LineEnding::None,
        };
        let body = if cr {
            &self.pending[..self.pending.len() - unit]
        } else {
            &self.pending[..]
        };
        let (text, _) = self.layout.text.encoding.decode_without_bom_handling(body);
        let first = self.done.lines.is_empty() && self.bom;
        self.done.lines.push(Line {
            text: text.into_owned(),
            ending,
            bom: first,
        });
        self.done.starts.push(next);
        self.pending.clear();
    }
}
