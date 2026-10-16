use std::ops::Range;
use std::sync::atomic::Ordering;

use super::{Dialect, reader, record_cells};
use crate::source::{RangeSource, ReadAt, Watch};

pub const STRIDE: u64 = 16;
pub const ROWS_BYTES: usize = 8 * 1024 * 1024;
const PROGRESS_EVERY: u64 = 4096;

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowSlice {
    pub rows: Vec<Vec<String>>,
    pub widest: usize,
}

pub fn index_records(
    read: &mut ReadAt<'_>,
    len: u64,
    dialect: Dialect,
    watch: &mut Watch<'_>,
) -> Result<Option<RecordIndex>, String> {
    let mut parser = reader(dialect, RangeSource::new(read, 0, len));
    let mut record = csv::ByteRecord::new();
    let mut index = RecordIndex::default();
    loop {
        if watch.cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        if !parser
            .read_byte_record(&mut record)
            .map_err(|error| error.to_string())?
        {
            break;
        }
        if index.records.is_multiple_of(STRIDE) {
            index
                .starts
                .push(record.position().map_or(0, csv::Position::byte));
        }
        index.records += 1;
        if index.records.is_multiple_of(PROGRESS_EVERY) {
            (watch.progress)(parser.position().byte());
        }
    }
    index.len = parser.position().byte();
    (watch.progress)(index.len);
    Ok(Some(index))
}

fn byte_range(index: &RecordIndex, rows: &Range<u64>) -> Result<(u64, usize, bool), String> {
    let start = index
        .block_start(rows.start / STRIDE)
        .ok_or_else(|| "The row index is out of date.".to_owned())?;
    let end = index
        .block_start((rows.end - 1) / STRIDE + 1)
        .unwrap_or(index.len);
    let len = usize::try_from(end.saturating_sub(start)).unwrap_or(usize::MAX);
    Ok((start, len.min(ROWS_BYTES), len > ROWS_BYTES))
}

pub fn read_rows(
    read: &mut ReadAt<'_>,
    index: &RecordIndex,
    dialect: Dialect,
    rows: Range<u64>,
) -> Result<RowSlice, String> {
    let rows = rows.start.min(index.records)..rows.end.min(index.records);
    if rows.is_empty() {
        return Ok(RowSlice::default());
    }
    let (start, len, is_cut) = byte_range(index, &rows)?;
    let bytes = read(start, len)?;
    let skip = rows.start - rows.start / STRIDE * STRIDE;
    let take = usize::try_from(rows.end - rows.start).unwrap_or(0);
    let mut slice = RowSlice::default();
    let mut record = csv::ByteRecord::new();
    let mut parser = reader(dialect, bytes.as_slice());
    let mut seen = 0;
    while slice.rows.len() < take
        && parser
            .read_byte_record(&mut record)
            .map_err(|error| error.to_string())?
    {
        let is_partial = is_cut && parser.position().byte() >= bytes.len() as u64;
        if seen >= skip && !(is_partial && seen > skip) {
            slice.widest = slice.widest.max(record.len());
            slice.rows.push(record_cells(&record));
        }
        seen += 1;
    }
    Ok(slice)
}
