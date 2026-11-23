//! Synthetic volumes whose folders the build cannot place: one orphan folder, and NTFS metadata
//! folders under `$Extend` (MFT record 11, which enumeration never returns).

use super::super::volume_io::{VolumeIo, enumerate_volume};
use super::super::{Enumeration, JournalPosition, NtfsError};
use super::{SyntheticRecord, append_record, journal_metadata};
use crate::pipe::CancelToken;

/// Folder 8 ("orphan") hangs under 99, which the MFT lacks; folder 30 ("docs") holds file 21
/// and subfolder 32 ("sub").
const ROWS: [(u64, u64, bool, &str); 4] = [
    (8, 99, true, "orphan"),
    (30, 5, true, "docs"),
    (21, 30, false, "x.txt"),
    (32, 30, true, "sub"),
];

/// `$Extend\$Deleted` (record 29, as on the VM), `$Extend\$RmMetadata` and its `$Txf` folder,
/// beside folder 30 ("docs") holding file 21 and subfolder 32, as in [`ROWS`].
const EXTEND_ROWS: [(u64, u64, bool, &str); 6] = [
    ((1 << 48) | 0x1D, (11 << 48) | 11, true, "$Deleted"),
    ((1 << 48) | 27, (11 << 48) | 11, true, "$RmMetadata"),
    ((1 << 48) | 28, (1 << 48) | 27, true, "$Txf"),
    (30, 5, true, "docs"),
    (21, 30, false, "x.txt"),
    (32, 30, true, "sub"),
];

/// Hands out its rows as one enumeration chunk.
struct SkippedFolderMft(&'static [(u64, u64, bool, &'static str)]);

impl VolumeIo for SkippedFolderMft {
    /// Supplies journal 7 at next USN 100.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        Ok(journal_metadata(7))
    }

    /// Emits every row in one chunk at cursor 0.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        if cursor != 0 {
            return Ok(None);
        }
        let mut bytes = 1u64.to_le_bytes().to_vec();
        for &(file_id, parent_id, folder, name) in self.0 {
            let spec = SyntheticRecord {
                file_id,
                parent_id,
                folder,
                name,
            };
            append_record(&mut bytes, spec);
        }
        Ok(Some(bytes))
    }

    /// Returns an empty journal batch.
    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        Ok(100i64.to_le_bytes().to_vec())
    }

    /// Gives every file a size of ten bytes.
    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        Ok(10)
    }
}

/// Builds volume C: through the production loop; folder 8 is skipped, `C:\docs\x.txt` is indexed.
pub fn skipped_folder_build() -> Enumeration {
    enumerate_volume(
        &mut SkippedFolderMft(&ROWS),
        'C',
        usize::MAX,
        &CancelToken::new(),
    )
    .unwrap()
}

/// Builds volume C: with the `$Extend` metadata folders skipped and `C:\docs\x.txt` indexed.
pub fn extend_folder_build() -> Enumeration {
    let mut volume = SkippedFolderMft(&EXTEND_ROWS);
    enumerate_volume(&mut volume, 'C', usize::MAX, &CancelToken::new()).unwrap()
}
