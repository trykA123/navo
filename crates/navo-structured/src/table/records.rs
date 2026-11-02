use std::ops::Range;
use std::sync::atomic::Ordering;

use super::Dialect;
use super::stream::{Next, Records, Row};
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
    pub is_cut: bool,
}

pub fn index_records(
    read: &mut ReadAt<'_>,
    len: u64,
    dialect: Dialect,
    watch: &mut Watch<'_>,
) -> Result<Option<RecordIndex>, String> {
    let mut records = Records::new(RangeSource::new(read, 0, len), 0, dialect);
    let mut index = RecordIndex::default();
    loop {
        if watch.cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Next::Record { start } = records.next(None)? else {
            break;
        };
        if index.records.is_multiple_of(STRIDE) {
            index.starts.push(start);
        }
        index.records += 1;
        if index.records.is_multiple_of(PROGRESS_EVERY) {
            (watch.progress)(records.offset());
        }
    }
    index.len = records.offset();
    (watch.progress)(index.len);
    Ok(Some(index))
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
    let block = rows.start / STRIDE;
    let start = index
        .block_start(block)
        .ok_or_else(|| "The row index is out of date.".to_owned())?;
    let source = RangeSource::new(read, start, index.len);
    let mut records = Records::new(source, start, dialect);
    for _ in block * STRIDE..rows.start {
        if let Next::End = records.next(None)? {
            return Ok(RowSlice::default());
        }
    }
    let mut slice = RowSlice::default();
    let mut used = 0;
    while (slice.rows.len() as u64) < rows.end - rows.start && !slice.is_cut {
        let mut row = Row::new(ROWS_BYTES - used);
        if let Next::End = records.next(Some(&mut row))? {
            break;
        }
        used += row.stored();
        slice.is_cut = row.is_clipped() || used >= ROWS_BYTES;
        slice.widest = slice.widest.max(row.fields);
        slice.rows.push(row.cells());
    }
    Ok(slice)
}
