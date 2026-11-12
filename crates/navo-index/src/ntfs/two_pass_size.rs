//! File-size rules shared by NTFS full-build adapters.

use crate::usn::UsnRecord;

use super::{Flags, NtfsError, VolumeIo, size_error};

pub(super) struct RecordSize {
    pub(super) bytes: Option<u64>,
    pub(super) unknown: u64,
}

/// Reads a record's bytes and unknown-size count, skipping vanished primary references.
/// Folders need no size I/O; other read failures retain the file with zero bytes.
pub(super) fn read_record_size(
    volume: &mut impl VolumeIo,
    record: &UsnRecord,
) -> Option<RecordSize> {
    let mut unknown = 0;
    let bytes = if record.flags.contains(Flags::FOLDER) {
        None
    } else {
        let size = match volume.file_size(record.file_id).map_err(size_error) {
            Err(NtfsError::InvalidResponse(_)) => volume.current_file_size(record.file_id),
            Err(NtfsError::FileGone) => return None,
            other => other,
        };
        Some(size.unwrap_or_else(|_| {
            unknown = 1;
            0
        }))
    };
    Some(RecordSize { bytes, unknown })
}
