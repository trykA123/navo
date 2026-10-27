use std::collections::{HashMap, HashSet};

use super::{
    Enumeration, Flags, JournalPosition, NtfsError, NtfsIndex, Pending, Resume, TableChange,
    UsnBatch, parse_batch, validate_journal_cursor,
};

pub(super) trait VolumeIo {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError>;
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError>;
    fn read_journal(&mut self, position: JournalPosition) -> Result<Vec<u8>, NtfsError>;
    fn file_size(&mut self, file_id: u128) -> Result<u64, NtfsError>;
    fn current_file_size(&mut self, file_id: u128) -> Result<u64, NtfsError> {
        self.file_size(file_id)
    }
}

pub(super) struct JournalState {
    position: JournalPosition,
    readable_from: i64,
}

impl JournalState {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, NtfsError> {
        if bytes.len() < 56 {
            return Err(NtfsError::InvalidResponse("truncated journal metadata"));
        }
        let journal_id = u64::from_le_bytes(field(bytes, 0)?);
        let first = i64::from_le_bytes(field(bytes, 8)?);
        let next_usn = i64::from_le_bytes(field(bytes, 16)?);
        let lowest = i64::from_le_bytes(field(bytes, 24)?);
        if first < 0 || lowest < 0 || next_usn < first.max(lowest) {
            return Err(NtfsError::InvalidResponse("invalid journal bounds"));
        }
        Ok(Self {
            position: JournalPosition {
                journal_id,
                next_usn,
            },
            readable_from: first.max(lowest),
        })
    }

    fn check(&self, saved: JournalPosition) -> Result<(), NtfsError> {
        if saved.resume(self.position.journal_id, self.readable_from) == Resume::Reenumerate
            || saved.next_usn > self.position.next_usn
        {
            return Err(NtfsError::Reenumerate);
        }
        Ok(())
    }
}

pub(super) fn enumerate_volume(
    volume: &mut impl VolumeIo,
    drive: char,
    limit: usize,
    cancel: &crate::pipe::CancelToken,
) -> Result<Enumeration, NtfsError> {
    let state = JournalState::parse(&volume.query_journal()?)?;
    let mut index = NtfsIndex::new(drive, state.position)?;
    let mut pending = Pending::default();
    let mut cursor = 0;
    let mut count = 0;
    let mut sizes_unknown = 0;
    while count < limit {
        if cancel.is_cancelled() {
            return Err(NtfsError::Cancelled);
        }
        let Some(bytes) = volume.enumerate(cursor)? else {
            break;
        };
        let batch = parse_batch(&bytes)?;
        let next = batch.next_usn.cast_unsigned();
        if next <= cursor {
            return Err(NtfsError::InvalidResponse(
                "enumeration cursor did not advance",
            ));
        }
        cursor = next;
        for record in batch.records.into_iter().take(limit - count) {
            count += 1;
            sizes_unknown += add_record(volume, &mut index, &mut pending, record)?;
        }
    }
    if cancel.is_cancelled() {
        return Err(NtfsError::Cancelled);
    }
    JournalState::parse(&volume.query_journal()?)?.check(state.position)?;
    Ok(Enumeration {
        index,
        unresolved: pending.finish(),
        sizes_unknown,
    })
}

fn add_record(
    volume: &mut impl VolumeIo,
    index: &mut NtfsIndex,
    pending: &mut Pending,
    record: crate::usn::UsnRecord,
) -> Result<u64, NtfsError> {
    let mut unknown = 0;
    let size = if record.flags.contains(Flags::FOLDER) {
        None
    } else {
        let size = match volume.file_size(record.file_id).map_err(size_error) {
            Err(NtfsError::InvalidResponse(_)) => volume.current_file_size(record.file_id),
            Err(NtfsError::FileGone) => return Ok(0),
            other => other,
        };
        Some(size.unwrap_or_else(|_| {
            unknown = 1;
            0
        }))
    };
    let file_id = record.file_id;
    pending.push(
        index,
        TableChange::Add {
            file_id: record.file_id,
            parent_id: record.parent_id,
            name: record.name,
            flags: record.flags,
        },
    )?;
    if let Some(bytes) = size {
        pending.push(index, TableChange::Size { file_id, bytes })?;
    }
    Ok(unknown)
}

pub(super) fn read_volume_journal(
    volume: &mut impl VolumeIo,
    saved: JournalPosition,
) -> Result<UsnBatch, NtfsError> {
    JournalState::parse(&volume.query_journal()?)?.check(saved)?;
    let bytes = volume.read_journal(saved).map_err(journal_error)?;
    let mut batch = parse_batch(&bytes)?;
    refresh_sizes(volume, &mut batch);
    validate_journal_cursor(&batch, saved)?;
    Ok(batch)
}

#[derive(Clone, Copy)]
enum FileSize {
    Known(u64),
    Gone,
    Unknown,
}

fn refresh_sizes(volume: &mut impl VolumeIo, batch: &mut UsnBatch) {
    const DELETE: u32 = 0x200;
    const SIZE_REASONS: u32 = 0x7 | 0x100 | 0x2000;
    let deleted: HashSet<_> = batch
        .records
        .iter()
        .filter(|record| record.reason & DELETE != 0)
        .map(|record| record.file_id)
        .collect();
    let mut sizes = HashMap::new();
    for record in &mut batch.records {
        if record.flags.contains(Flags::FOLDER)
            || deleted.contains(&record.file_id)
            || record.reason & SIZE_REASONS == 0
        {
            continue;
        }
        let size = match sizes.entry(record.file_id) {
            std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let size = match volume.current_file_size(record.file_id).map_err(size_error) {
                    Ok(size) => FileSize::Known(size),
                    Err(NtfsError::FileGone) => FileSize::Gone,
                    Err(_) => FileSize::Unknown,
                };
                *entry.insert(size)
            }
        };
        record.size = match size {
            FileSize::Known(bytes) => Some(bytes),
            FileSize::Gone => {
                record.reason = DELETE;
                None
            }
            FileSize::Unknown => None,
        };
    }
}

pub(super) fn size_error(error: NtfsError) -> NtfsError {
    match error {
        NtfsError::Io { ref source, .. }
            if matches!(source.raw_os_error(), Some(2 | 3 | 87 | 1920)) =>
        {
            NtfsError::FileGone
        }
        other => other,
    }
}

pub(super) fn journal_error(error: NtfsError) -> NtfsError {
    const JOURNAL_ENTRY_DELETED: i32 = 1181;
    match error {
        NtfsError::Io { source, .. } if source.raw_os_error() == Some(JOURNAL_ENTRY_DELETED) => {
            NtfsError::Reenumerate
        }
        other => other,
    }
}

pub(super) fn field<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], NtfsError> {
    bytes
        .get(offset..)
        .and_then(|tail| tail.get(..N))
        .and_then(|value| value.try_into().ok())
        .ok_or(NtfsError::InvalidResponse("truncated metadata field"))
}

pub(super) fn volume_path(drive: char) -> Result<Vec<u16>, NtfsError> {
    if !drive.is_ascii_alphabetic() {
        return Err(NtfsError::InvalidVolume);
    }
    Ok(format!("\\\\.\\{}:\0", drive.to_ascii_uppercase())
        .encode_utf16()
        .collect())
}

pub(super) fn enum_request(cursor: u64) -> Vec<u8> {
    let mut bytes = vec![0; 24];
    bytes[..8].copy_from_slice(&cursor.to_le_bytes());
    bytes[16..24].copy_from_slice(&i64::MAX.to_le_bytes());
    bytes
}

pub(super) fn journal_request(position: JournalPosition) -> Vec<u8> {
    let mut bytes = vec![0; 40];
    bytes[..8].copy_from_slice(&position.next_usn.to_le_bytes());
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes[32..40].copy_from_slice(&position.journal_id.to_le_bytes());
    bytes
}
