use std::io::BufRead;
use std::ops::Range;
use std::sync::atomic::Ordering;

use super::{Dialect, reader, record_cells};
use crate::source::{RangeSource, ReadAt, Watch};

pub const STRIDE: u64 = 16;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordIndex {
    pub starts: Vec<u64>,
    pub records: u64,
    pub len: u64,
}

impl RecordIndex {
    #[must_use]
    pub fn size_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.starts.capacity() * std::mem::size_of::<u64>()
    }

    fn block_start(&self, block: u64) -> Option<u64> {
        usize::try_from(block)
            .ok()
            .and_then(|block| self.starts.get(block).copied())
    }
}

#[derive(Default)]
struct Scan {
    index: RecordIndex,
    in_quotes: bool,
    has_content: bool,
}

impl Scan {
    fn feed(&mut self, chunk: &[u8], offset: u64) {
        for (at, byte) in chunk.iter().enumerate() {
            match byte {
                b'\n' if !self.in_quotes => self.end_record(),
                b'\r' if !self.has_content => {}
                _ => self.content(*byte, offset + at as u64),
            }
        }
    }

    fn content(&mut self, byte: u8, position: u64) {
        if !self.has_content {
            self.has_content = true;
            if self.index.records.is_multiple_of(STRIDE) {
                self.index.starts.push(position);
            }
        }
        if byte == b'"' {
            self.in_quotes = !self.in_quotes;
        }
    }

    fn end_record(&mut self) {
        if self.has_content {
            self.index.records += 1;
            self.has_content = false;
        }
    }
}

pub fn index_records(
    read: &mut ReadAt<'_>,
    len: u64,
    watch: &mut Watch<'_>,
) -> Result<Option<RecordIndex>, String> {
    let mut scan = Scan::default();
    let mut source = RangeSource::new(read, 0, len);
    let mut offset = 0u64;
    loop {
        if watch.cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let chunk = source.fill_buf().map_err(|error| error.to_string())?;
        if chunk.is_empty() {
            break;
        }
        let count = chunk.len();
        scan.feed(chunk, offset);
        source.consume(count);
        offset += count as u64;
        (watch.progress)(offset);
    }
    scan.end_record();
    scan.index.len = offset;
    Ok(Some(scan.index))
}

pub fn read_rows(
    read: &mut ReadAt<'_>,
    index: &RecordIndex,
    dialect: Dialect,
    rows: Range<u64>,
) -> Result<Vec<Vec<String>>, String> {
    let rows = rows.start.min(index.records)..rows.end.min(index.records);
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let block = rows.start / STRIDE;
    let start = index
        .block_start(block)
        .ok_or_else(|| "The row index is out of date.".to_owned())?;
    let end = index
        .block_start((rows.end - 1) / STRIDE + 1)
        .unwrap_or(index.len);
    let len = usize::try_from(end.saturating_sub(start)).map_err(|error| error.to_string())?;
    let bytes = read(start, len)?;
    let bytes = if start == 0 {
        bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)
    } else {
        &bytes
    };
    let skip = usize::try_from(rows.start - block * STRIDE).unwrap_or(0);
    let take = usize::try_from(rows.end - rows.start).unwrap_or(0);
    let mut cells = Vec::with_capacity(take);
    let mut record = csv::ByteRecord::new();
    let mut parser = reader(dialect, bytes);
    let mut seen = 0;
    while cells.len() < take
        && parser
            .read_byte_record(&mut record)
            .map_err(|error| error.to_string())?
    {
        if seen >= skip {
            cells.push(record_cells(&record));
        }
        seen += 1;
    }
    Ok(cells)
}
