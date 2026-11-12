//! Synthetic MFT fixtures for NTFS enumeration, layout, and scale tests.

#![allow(
    clippy::unwrap_used,
    reason = "test-support fixtures fail immediately on invalid data or broken coordination"
)]

#[path = "deep_mft.rs"]
mod deep_mft;
pub use deep_mft::DeepMft;

use super::volume_io::{VolumeIo, enumerate_volume};
use super::{Enumeration, JournalPosition, NtfsError};
use crate::pipe::CancelToken;

pub(super) const CHUNK_RECORDS: u32 = 512;
pub(super) const MAX_CHUNK_BYTES: usize = 64 * 1024;

/// A synthetic MFT that hands out one 64 KiB enumeration buffer per call, like the volume does.
pub struct MftFixture {
    entries: u32,
    journal_id: u64,
}

impl MftFixture {
    /// Creates a flat synthetic MFT with `entries - 1` files under root `5`.
    pub fn new(entries: u32, journal_id: u64) -> Self {
        Self {
            entries,
            journal_id,
        }
    }

    /// Reads the full synthetic volume through the production enumeration loop.
    pub fn read(mut self) -> Enumeration {
        enumerate_volume(&mut self, 'C', usize::MAX, &CancelToken::new()).unwrap()
    }
}

impl VolumeIo for MftFixture {
    /// Supplies journal metadata with `self.journal_id` and next USN `100`.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        Ok(journal_metadata(self.journal_id))
    }

    /// Emits up to 512 records starting at `cursor` (1 when `cursor == 0`) within 64 KiB.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        let start = if cursor == 0 {
            1
        } else {
            let Ok(start) = u32::try_from(cursor) else {
                return Ok(None);
            };
            start
        };
        if start >= self.entries {
            return Ok(None);
        }
        let end = start.saturating_add(CHUNK_RECORDS).min(self.entries);
        let entries = self.entries;
        let bytes = stream_chunk(start, end, |buf, number| {
            let name = if number == entries - 1 {
                "needle.txt".into()
            } else {
                format!("entry-{number:07}.txt")
            };
            append_record(
                buf,
                SyntheticRecord {
                    file_id: u64::from(number) + 100,
                    parent_id: 5,
                    folder: false,
                    name: &name,
                },
            );
        });
        Ok(Some(bytes))
    }

    /// Returns an empty journal batch.
    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        Ok(100i64.to_le_bytes().to_vec())
    }

    /// Gives every synthetic file a size of one byte.
    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        Ok(1)
    }
}

/// Builds a 56-byte synthetic `FSCTL_QUERY_USN_JOURNAL` response.
pub(super) fn journal_metadata(journal_id: u64) -> Vec<u8> {
    let mut bytes = vec![0; 56];
    bytes[..8].copy_from_slice(&journal_id.to_le_bytes());
    bytes[16..24].copy_from_slice(&100i64.to_le_bytes());
    bytes
}

/// Encodes one 64 KiB-bounded enumeration chunk covering record indices `start..end`.
pub(super) fn stream_chunk<F>(start: u32, end: u32, mut emit: F) -> Vec<u8>
where
    F: FnMut(&mut Vec<u8>, u32),
{
    let mut bytes = u64::from(end).to_le_bytes().to_vec();
    for number in start..end {
        emit(&mut bytes, number);
    }
    assert!(bytes.len() <= MAX_CHUNK_BYTES);
    bytes
}

/// Fields encoded into one synthetic USN v2 record.
pub(super) struct SyntheticRecord<'a> {
    pub(super) file_id: u64,
    pub(super) parent_id: u64,
    pub(super) folder: bool,
    pub(super) name: &'a str,
}

/// Appends one USN v2 record with the given reference, parent, folder flag and UTF-16 name.
pub(super) fn append_record(bytes: &mut Vec<u8>, spec: SyntheticRecord<'_>) {
    let utf16_len = spec.name.encode_utf16().count() * 2;
    let length = (60 + utf16_len).next_multiple_of(8);
    let start = bytes.len();
    bytes.resize(start + length, 0);
    let record = &mut bytes[start..start + length];
    record[..4].copy_from_slice(&(length as u32).to_le_bytes());
    record[4..6].copy_from_slice(&2u16.to_le_bytes());
    record[8..16].copy_from_slice(&spec.file_id.to_le_bytes());
    record[16..24].copy_from_slice(&spec.parent_id.to_le_bytes());
    record[24..32].copy_from_slice(&90i64.to_le_bytes());
    if spec.folder {
        record[52..56].copy_from_slice(&0x10u32.to_le_bytes());
    }
    record[56..58].copy_from_slice(&(utf16_len as u16).to_le_bytes());
    record[58..60].copy_from_slice(&60u16.to_le_bytes());
    let (pairs, _) = record[60..].as_chunks_mut::<2>();
    for (pair, unit) in pairs.iter_mut().zip(spec.name.encode_utf16()) {
        pair.copy_from_slice(&unit.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usn::{UsnRecord, parse_batch};

    pub(super) struct Streamed {
        pub(super) chunks: Vec<Vec<u8>>,
        pub(super) records: Vec<UsnRecord>,
        pub(super) eofs: usize,
    }

    /// Reads one complete enumeration pass from cursor 0 to EOF.
    pub(super) fn collect_stream(volume: &mut impl VolumeIo) -> Streamed {
        let mut cursor = 0;
        let mut chunks = Vec::new();
        let mut records = Vec::new();
        let mut eofs = 0;
        loop {
            match volume.enumerate(cursor).unwrap() {
                None => {
                    eofs += 1;
                    break;
                }
                Some(bytes) => {
                    assert!(bytes.len() <= MAX_CHUNK_BYTES);
                    let batch = parse_batch(&bytes).unwrap();
                    cursor = batch.next_usn.cast_unsigned();
                    chunks.push(bytes);
                    records.extend(batch.records);
                }
            }
        }
        Streamed {
            chunks,
            records,
            eofs,
        }
    }

    #[test]
    fn mft_fixture_replay_returns_identical_records_bounded_chunks_and_eofs() {
        let mut fixture = MftFixture::new(1_200, 7);
        let first = collect_stream(&mut fixture);
        let second = collect_stream(&mut fixture);
        assert_eq!(first.eofs, 1);
        assert_eq!(second.eofs, 1);
        assert_eq!(first.records.len(), 1_199);
        assert_eq!(second.records, first.records);
        assert_eq!(second.chunks, first.chunks);

        let result = MftFixture::new(1_200, 7).read();
        assert_eq!(result.index.table.len(), 1_200);
        assert_eq!(result.index.table.folder_count(), 1);
        let needle = result.index.ids[&1_299];
        assert_eq!(result.index.table.name(needle), Some("needle.txt"));
    }
}
