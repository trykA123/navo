//! Streaming deep-tree MFT fixture with four-deep folders and 30% early files.

use super::super::volume_io::{VolumeIo, enumerate_volume};
use super::super::{Enumeration, JournalPosition, NtfsError};
use super::{CHUNK_RECORDS, SyntheticRecord, append_record, journal_metadata, stream_chunk};
use crate::pipe::CancelToken;

/// Synthetic MFT streaming four-deep folder trees (10% folders) with 30% early files.
pub struct DeepMft {
    files: u32,
    ordered: bool,
}

impl DeepMft {
    /// Streams `files` files plus 10% directories, with 30% of files preceding delayed trees.
    pub fn new(files: u32) -> Self {
        Self {
            files,
            ordered: false,
        }
    }

    /// Streams the same four-deep tree in parent-first order for baseline builds.
    pub fn ordered(files: u32) -> Self {
        Self {
            files,
            ordered: true,
        }
    }

    /// Reads the entire synthetic volume through the production enumeration loop.
    pub fn read(mut self) -> Enumeration {
        enumerate_volume(&mut self, 'C', usize::MAX, &CancelToken::new()).unwrap()
    }

    /// Returns a record's `(file_id, parent_id, is_folder)` at stream position `at`.
    fn item(&self, at: u32) -> (u64, u64, bool) {
        let folders = self.files / 10;
        let delayed_files = self.files * 3 / 10;
        let delayed_folders = folders * 3 / 10;
        let early_end = delayed_files + folders - delayed_folders;
        let late_files_end = self.files + folders - delayed_folders;
        let (number, folder) = if self.ordered {
            if at < folders {
                (self.files + at, true)
            } else {
                (at - folders, false)
            }
        } else if at < delayed_files {
            (at, false)
        } else if at < early_end {
            (self.files + delayed_folders + at - delayed_files, true)
        } else if at < late_files_end {
            (delayed_files + at - early_end, false)
        } else {
            (self.files + at - late_files_end, true)
        };
        let parent = if folder {
            if (number - self.files).is_multiple_of(4) {
                5
            } else {
                number + 99
            }
        } else {
            self.files + number / 40 * 4 + 3 + 100
        };
        (u64::from(number) + 100, u64::from(parent), folder)
    }
}

impl VolumeIo for DeepMft {
    /// Supplies stable journal metadata for the synthetic volume.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        Ok(journal_metadata(7))
    }

    /// Emits up to 512 records starting at `cursor` within the 64 KiB buffer limit.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        let total = self.files + self.files / 10;
        let Ok(start) = u32::try_from(cursor) else {
            return Ok(None);
        };
        if start >= total {
            return Ok(None);
        }
        let end = start.saturating_add(CHUNK_RECORDS).min(total);
        let bytes = stream_chunk(start, end, |buf, at| {
            let (file_id, parent_id, folder) = self.item(at);
            let name = format!("entry-{:07}.txt", file_id - 100);
            append_record(
                buf,
                SyntheticRecord {
                    file_id,
                    parent_id,
                    folder,
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

#[cfg(test)]
mod tests {
    use super::super::tests::collect_stream;
    use super::*;

    #[test]
    fn deep_mft_replay_streams_four_deep_tree_and_restarts_at_zero() {
        let mut fixture = DeepMft::new(1_200);
        let first = collect_stream(&mut fixture);
        let second = collect_stream(&mut fixture);
        assert_eq!(first.eofs, 1);
        assert_eq!(second.eofs, 1);
        assert_eq!(first.records.len(), 1_320);
        assert_eq!(second.records, first.records);
        assert_eq!(second.chunks, first.chunks);

        let result = DeepMft::new(1_200).read();
        assert!(result.unresolved.is_empty());
        assert_eq!(result.index.table.len(), 1_321);
        assert_eq!(result.index.table.folder_count(), 121);
        assert_eq!(result.blocked_peak, 360);
        let sample = result.index.ids[&100];
        let path = result.index.table.path_with(sample, '\\').unwrap();
        assert_eq!(path.matches('\\').count(), 5);

        let ordered = DeepMft::ordered(400).read();
        assert_eq!(ordered.blocked_peak, 0);
        assert_eq!(ordered.index.table.len(), 441);
        assert_eq!(ordered.index.table.folder_count(), 41);
    }
}
