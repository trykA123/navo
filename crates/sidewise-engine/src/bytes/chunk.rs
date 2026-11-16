use std::io::{self, Read};

use blake3::Hasher;
use fastcdc::v2020::StreamCDC;

use crate::anchor::{Anchor, Candidate};
use crate::model::{Budget, CancelToken, Reason};

use super::budget::{CHUNK_BYTES, Reporter};

const MIN_CHUNK: usize = 2 * 1024;
const AVG_CHUNK: usize = 8 * 1024;
const MAX_CHUNK: usize = 64 * 1024;

struct CancelReader {
    inner: Box<dyn Read + Send>,
    cancel: CancelToken,
}

impl Read for CancelReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::other("cancelled"));
        }
        self.inner.read(buffer)
    }
}

#[derive(Debug)]
pub(super) struct Chunks {
    pub(super) entries: Vec<Candidate<[u8; 32]>>,
    pub(super) len: u64,
}

impl Chunks {
    pub(super) fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    pub(super) fn length_at(&self, offset: u64) -> Option<u64> {
        let index = self
            .entries
            .binary_search_by_key(&offset, |chunk| chunk.position)
            .ok()?;
        let end = self
            .entries
            .get(index + 1)
            .map_or(self.len, |chunk| chunk.position);
        end.checked_sub(offset)
    }
}

pub(super) fn chunks(
    reader: Box<dyn Read + Send>,
    reporter: &mut Reporter<'_>,
    budget: Budget,
    retained: usize,
) -> Result<Option<Chunks>, Reason> {
    let reader = CancelReader {
        inner: reader,
        cancel: reporter.cancel.clone(),
    };
    let chunker = StreamCDC::new(reader, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK);
    let mut entries = Vec::new();
    let mut len = 0_u64;
    for chunk in chunker {
        if reporter.cancelled() {
            return Err(Reason::Stopped);
        }
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) if reporter.cancelled() => return Err(Reason::Stopped),
            Err(_) => return Err(Reason::ReadError),
        };
        if !reserve(&mut entries, retained, budget)? {
            return Ok(None);
        }
        let length = u64::try_from(chunk.length).map_err(|_| Reason::ReadError)?;
        let hash = *Hasher::new().update(&chunk.data).finalize().as_bytes();
        entries.push(Candidate {
            key: hash,
            position: chunk.offset,
        });
        len = len.checked_add(length).ok_or(Reason::ReadError)?;
        reporter.add(length);
    }
    Ok(Some(Chunks { entries, len }))
}

fn reserve(
    entries: &mut Vec<Candidate<[u8; 32]>>,
    retained: usize,
    budget: Budget,
) -> Result<bool, Reason> {
    if entries.len() < entries.capacity() {
        return Ok(true);
    }
    let capacity = entries.capacity().max(2).saturating_mul(2);
    let (records, anchors) = construction_bytes(retained, capacity);
    let previous = bytes(entries.capacity(), CHUNK_BYTES);
    if records.saturating_add(anchors).saturating_add(previous) > budget.bytes() / 2 {
        return Ok(false);
    }
    entries
        .try_reserve_exact(capacity.saturating_sub(entries.len()))
        .map_err(|_| Reason::ReadError)?;
    let (records, anchors) = construction_bytes(retained, entries.capacity());
    #[cfg(test)]
    super::retained::record(records.saturating_add(anchors).saturating_add(previous), 0);
    Ok(records.saturating_add(anchors) <= budget.bytes() / 2)
}

pub(super) fn construction_bytes(left: usize, right: usize) -> (u64, u64) {
    let records = bytes(left.saturating_add(right), CHUNK_BYTES);
    let count = left.min(right);
    let capacity = if count == 0 {
        0
    } else {
        count
            .max(4)
            .checked_next_power_of_two()
            .unwrap_or(usize::MAX)
    };
    let pointers = bytes(
        left.saturating_add(right),
        std::mem::size_of::<usize>() as u64,
    );
    let growing = 3 * std::mem::size_of::<Anchor>() + 2 * std::mem::size_of::<usize>();
    let anchors = pointers
        .saturating_add(bytes(capacity, growing as u64))
        .saturating_add(bytes(count, std::mem::size_of::<Option<usize>>() as u64));
    (records, anchors)
}

fn bytes(count: usize, size: u64) -> u64 {
    u64::try_from(count)
        .unwrap_or(u64::MAX)
        .saturating_mul(size)
}
