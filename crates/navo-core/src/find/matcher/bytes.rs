//! Streaming raw-byte scanner for `hex:` find queries.

use std::fmt::Write as _;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};

use memchr::memmem;

use super::{CHUNK, FileHits, HIT_LIMIT, LineHit, Skip, read_chunk};

const ROW_BYTES: usize = 16;
const ROW_BYTES_U64: u64 = 16;

/// Scans `reader` for raw byte matches of `needle` without skipping binary files.
pub(super) fn scan_bytes(
    mut reader: impl Read,
    needle: &[u8],
    cancel: &AtomicBool,
) -> io::Result<FileHits> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(FileHits {
            skipped: Some(Skip::Cancelled),
            ..FileHits::default()
        });
    }
    if needle.is_empty() {
        return Ok(FileHits::default());
    }
    let mut scanner = ByteScanner::new(needle, cancel);
    scanner.run(&mut reader)?;
    Ok(scanner.found)
}

struct ByteScanner<'a> {
    needle: &'a [u8],
    cancel: &'a AtomicBool,
    buffer: Vec<u8>,
    filled: usize,
    base: u64,
    next_search: u64,
    found: FileHits,
}

impl<'a> ByteScanner<'a> {
    fn new(needle: &'a [u8], cancel: &'a AtomicBool) -> Self {
        let size = CHUNK.max(needle.len().next_multiple_of(ROW_BYTES) + ROW_BYTES);
        Self {
            needle,
            cancel,
            buffer: vec![0; size],
            filled: 0,
            base: 0,
            next_search: 0,
            found: FileHits::default(),
        }
    }

    fn run(&mut self, reader: &mut impl Read) -> io::Result<()> {
        loop {
            if self.cancelled() {
                return Ok(());
            }
            let eof = self.fill(reader)?;
            if self.cancelled() || self.filled == 0 {
                return Ok(());
            }
            let after_last = self.scan_buffer();
            if self.cancelled() || eof {
                return Ok(());
            }
            self.advance(after_last);
        }
    }

    fn cancelled(&mut self) -> bool {
        if !self.cancel.load(Ordering::Relaxed) {
            return false;
        }
        self.found.skipped = Some(Skip::Cancelled);
        true
    }

    fn fill(&mut self, reader: &mut impl Read) -> io::Result<bool> {
        while self.filled < self.buffer.len() && !self.cancel.load(Ordering::Relaxed) {
            let count = read_chunk(reader, &mut self.buffer[self.filled..])?;
            if count == 0 {
                return Ok(true);
            }
            self.filled += count;
        }
        Ok(false)
    }

    fn scan_buffer(&mut self) -> u64 {
        let start = usize::try_from(self.next_search.saturating_sub(self.base))
            .unwrap_or(self.filled)
            .min(self.filled);
        let data = &self.buffer[..self.filled];
        let needle_len = u64::try_from(self.needle.len()).unwrap_or(u64::MAX);
        let mut after_last = 0u64;
        for rel in memmem::find_iter(&data[start..], self.needle) {
            if self.cancel.load(Ordering::Relaxed) {
                return after_last;
            }
            let pos = start + rel;
            let offset = self
                .base
                .saturating_add(u64::try_from(pos).unwrap_or(u64::MAX));
            after_last = offset.saturating_add(needle_len);
            record_hit(&mut self.found, data, pos, offset);
        }
        after_last
    }

    fn advance(&mut self, after_last: u64) {
        let overlap = self.needle.len() - 1;
        let tail_offset = u64::try_from(self.filled.saturating_sub(overlap)).unwrap_or(u64::MAX);
        let tail_start = self.base.saturating_add(tail_offset);
        self.next_search = tail_start.max(after_last);
        let keep_from = self.next_search & !(ROW_BYTES_U64 - 1);
        let drop_bytes =
            usize::try_from(keep_from.saturating_sub(self.base)).unwrap_or(self.filled);
        self.buffer.copy_within(drop_bytes..self.filled, 0);
        self.filled -= drop_bytes;
        self.base = keep_from;
    }
}

fn record_hit(found: &mut FileHits, data: &[u8], pos: usize, offset: u64) {
    found.total = found.total.saturating_add(1);
    if found.hits.len() >= HIT_LIMIT {
        return;
    }
    let row_start = pos & !(ROW_BYTES - 1);
    let row_end = (row_start + ROW_BYTES).min(data.len());
    let row_offset = offset & !(ROW_BYTES_U64 - 1);
    let column = u32::try_from(offset % ROW_BYTES_U64 + 1).unwrap_or(1);
    found.hits.push(LineHit {
        line: offset / ROW_BYTES_U64 + 1,
        column,
        text: format_hex_row(row_offset, &data[row_start..row_end]),
    });
}

fn format_hex_row(row_offset: u64, row: &[u8]) -> String {
    let mut text = String::with_capacity(9 + row.len() * 3);
    let _ = write!(text, "{row_offset:08X} ");
    for byte in row {
        let _ = write!(text, " {byte:02X}");
    }
    text
}
