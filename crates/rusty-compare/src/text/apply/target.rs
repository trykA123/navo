use std::io::Write;
use std::ops::Range;

use rusty_core::encoding::{TextEncoding, encode};
use rusty_core::paging::{ByteSource, TextLayout, read_full};

use super::super::side::LineEnding;
use super::{ApplyError, TargetFacts};

pub(super) const BUFFER: usize = 64 * 1024;

pub(super) struct Target<'a> {
    pub(super) source: &'a dyn ByteSource,
    pub(super) layout: TextLayout,
    pub(super) plain: TextEncoding,
    pub(super) dominant: Vec<u8>,
    pub(super) len: u64,
    pub(super) bom_len: u64,
    pub(super) final_newline: bool,
}

impl<'a> Target<'a> {
    pub(super) fn new(source: &'a dyn ByteSource, facts: TargetFacts) -> Result<Self, ApplyError> {
        let layout = source.layout();
        let len = source.len();
        let bom_len = if layout.text.bom {
            u64::try_from(layout.bom_len()).map_err(|_| ApplyError::Range)?
        } else {
            0
        };
        if len < bom_len {
            return Err(ApplyError::Range);
        }
        let plain = TextEncoding {
            bom: false,
            ..layout.text
        };
        let dominant = if facts.dominant == LineEnding::None {
            LineEnding::Lf
        } else {
            facts.dominant
        };
        let dominant = encode(dominant.as_str(), &plain)?;
        Ok(Self {
            source,
            layout,
            plain,
            dominant,
            len,
            bom_len,
            final_newline: facts.final_newline,
        })
    }
}

pub(super) struct Writer<'a> {
    out: &'a mut dyn Write,
    layout: TextLayout,
    dominant: &'a [u8],
    tail: [u8; 2],
    filled: usize,
}

impl<'a> Writer<'a> {
    pub(super) fn new(out: &'a mut dyn Write, layout: TextLayout, dominant: &'a [u8]) -> Self {
        Self {
            out,
            layout,
            dominant,
            tail: [0; 2],
            filled: 0,
        }
    }

    pub(super) fn write_all(&mut self, bytes: &[u8]) -> Result<(), ApplyError> {
        self.out.write_all(bytes).map_err(ApplyError::Write)?;
        self.remember(bytes);
        Ok(())
    }

    pub(super) fn is_open(&self) -> bool {
        if self.filled == 0 {
            return false;
        }
        let unit = self.layout.unit();
        self.filled < unit || !self.layout.is_lf(&self.tail[..unit])
    }

    pub(super) fn finish_line(&mut self) -> Result<(), ApplyError> {
        if !self.is_open() {
            return Ok(());
        }
        let dominant = self.dominant;
        self.write_all(dominant)
    }

    pub(super) fn end_line(&mut self) -> Result<(), ApplyError> {
        let dominant = self.dominant;
        self.write_all(dominant)
    }

    fn remember(&mut self, bytes: &[u8]) {
        let unit = self.layout.unit();
        if bytes.len() >= unit {
            self.tail[..unit].copy_from_slice(&bytes[bytes.len() - unit..]);
            self.filled = unit;
            return;
        }
        if bytes.is_empty() {
            return;
        }
        let mut joined = [0u8; 4];
        let total = self.filled + bytes.len();
        joined[..self.filled].copy_from_slice(&self.tail[..self.filled]);
        joined[self.filled..total].copy_from_slice(bytes);
        let keep = total.min(unit);
        self.tail[..keep].copy_from_slice(&joined[total - keep..total]);
        self.filled = keep;
    }
}

pub(super) fn copy_target(
    target: &Target<'_>,
    range: Range<u64>,
    writer: &mut Writer<'_>,
    buffer: &mut [u8],
) -> Result<(), ApplyError> {
    let mut at = range.start;
    while at < range.end {
        let read = read_block(target.source, at, range.end, buffer)?;
        writer.write_all(&buffer[..read])?;
        at = at.saturating_add(u64::try_from(read).map_err(|_| ApplyError::Range)?);
    }
    Ok(())
}

pub(super) fn copy_raw(
    source: &dyn ByteSource,
    range: Range<u64>,
    out: &mut dyn Write,
    buffer: &mut [u8],
) -> Result<(), ApplyError> {
    let mut at = range.start;
    while at < range.end {
        let read = read_block(source, at, range.end, buffer)?;
        out.write_all(&buffer[..read]).map_err(ApplyError::Write)?;
        at = at.saturating_add(u64::try_from(read).map_err(|_| ApplyError::Range)?);
    }
    Ok(())
}

pub(super) fn read_block(
    source: &dyn ByteSource,
    at: u64,
    end: u64,
    buffer: &mut [u8],
) -> Result<usize, ApplyError> {
    let remaining = end.checked_sub(at).ok_or(ApplyError::Range)?;
    let capacity = u64::try_from(buffer.len()).map_err(|_| ApplyError::Range)?;
    let want = usize::try_from(remaining.min(capacity)).map_err(|_| ApplyError::Range)?;
    let read = read_full(source, at, &mut buffer[..want]).map_err(ApplyError::Read)?;
    if read != want {
        return Err(ApplyError::Read("the file ended early".to_owned()));
    }
    Ok(read)
}
