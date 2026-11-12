//! Volume enumeration and journal I/O for the NTFS index.

use std::collections::{HashMap, HashSet};

use super::{
    Enumeration, Flags, JournalPosition, NtfsError, NtfsIndex, Pending, Resume, SizeHint,
    TableChange, UsnBatch, parse_batch, validate_journal_cursor,
};

#[path = "two_pass_size.rs"]
mod two_pass_size;

#[cfg(test)]
thread_local! {
    static RESERVE_FAILURES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
    enumerate_volume_sized(volume, drive, limit, SizeHint::default(), cancel)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the volume, limit, hint and cancel token are separate build inputs"
)]
pub(super) fn enumerate_volume_sized(
    volume: &mut impl VolumeIo,
    drive: char,
    limit: usize,
    hint: SizeHint,
    cancel: &crate::pipe::CancelToken,
) -> Result<Enumeration, NtfsError> {
    let state = JournalState::parse(&volume.query_journal()?)?;
    let mut index = NtfsIndex::new(drive, state.position)?;
    let sized = reserve_for_build(&mut index, hint, drive)?;
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
    // A sized build keeps its spare until the service has dropped the old index.
    if !sized {
        index.table.trim_slack();
    }
    index.ids.shrink_to_fit();
    let blocked_peak = pending.blocked_peak;
    Ok(Enumeration {
        index,
        unresolved: pending.finish(),
        sizes_unknown,
        blocked_peak,
    })
}

/// Reserves the hint's total capacities, falling back to a fresh unsized index on allocation failure.
/// Returns whether the reserve holds.
fn reserve_for_build(
    index: &mut NtfsIndex,
    hint: SizeHint,
    drive: char,
) -> Result<bool, NtfsError> {
    if hint == SizeHint::default() {
        return Ok(false);
    }
    let reserved = index
        .table
        .try_reserve_total(hint.entries, hint.folders, hint.name_bytes);
    if let Err(error) = reserved {
        tracing::warn!(%error, drive = %drive, "index reserve failed; building unsized");
        #[cfg(test)]
        RESERVE_FAILURES.with(|count| count.set(count.get() + 1));
        *index = NtfsIndex::new(drive, index.position)?;
        return Ok(false);
    }
    Ok(true)
}

/// Queues a record and its bytes, returning the unknown-size count or an apply error.
fn add_record(
    volume: &mut impl VolumeIo,
    index: &mut NtfsIndex,
    pending: &mut Pending,
    record: crate::usn::UsnRecord,
) -> Result<u64, NtfsError> {
    let Some(size) = two_pass_size::read_record_size(volume, &record) else {
        return Ok(0);
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
    if let Some(bytes) = size.bytes {
        pending.push(index, TableChange::Size { file_id, bytes })?;
    }
    Ok(size.unknown)
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

/// MFT record slots (used and free) from an `NTFS_VOLUME_DATA_BUFFER`: an upper estimate of the file count.
pub(super) fn mft_records(data: &[u8]) -> Result<u64, NtfsError> {
    let record_size = u32::from_le_bytes(field(data, 48)?);
    if !(512..=65536).contains(&record_size) {
        return Err(NtfsError::InvalidResponse("invalid MFT record size"));
    }
    let valid = u64::try_from(i64::from_le_bytes(field(data, 56)?))
        .map_err(|_| NtfsError::InvalidResponse("invalid MFT length"))?;
    Ok(valid / u64::from(record_size))
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

#[cfg(test)]
mod sized_tests {
    #![allow(
        clippy::unwrap_used,
        clippy::print_stdout,
        reason = "sized-build tests fail on broken fixtures and print the measured peak"
    )]

    use super::super::test_support::MftFixture;
    use super::{RESERVE_FAILURES, enumerate_volume_sized};
    use crate::heap_probe::{MIB, Probe, fail_allocations_from};
    use crate::ntfs::{NtfsIndex, SizeHint};
    use crate::pipe::CancelToken;

    const ENTRIES: u32 = 50_000;
    const BIG: u32 = 1_100_000;

    fn hint_for(index: &NtfsIndex) -> SizeHint {
        let entries = index.table.slots() as usize;
        let folders = index.table.folder_count();
        let names = index.table.live_name_bytes();
        let entries = entries + entries / 16;
        SizeHint {
            entries,
            folders: (folders + folders / 16).min(entries),
            name_bytes: names + names / 16,
        }
    }

    /// Heap bytes the index holds, measured by dropping it.
    fn held(index: NtfsIndex, probe: &Probe) -> usize {
        let before = probe.read().retained;
        drop(index);
        (before - probe.read().retained).unsigned_abs()
    }

    #[test]
    fn a_sized_build_of_one_point_one_million_entries_stays_within_12_bytes() {
        let first = MftFixture::new(BIG, 7).read();
        let hint = hint_for(&first.index);
        drop(first);
        let probe = Probe::start();
        let result = enumerate_volume_sized(
            &mut MftFixture::new(BIG, 7),
            'C',
            usize::MAX,
            hint,
            &CancelToken::new(),
        )
        .unwrap();
        let measured = probe.read();
        let index = held(result.index, &probe);
        let transient = measured.peak.saturating_sub(index);
        let per_entry = transient / BIG as usize;
        println!(
            "sized build: peak {} KiB, held {} KiB, transient {transient} B, per-entry {per_entry} B",
            measured.peak >> 10,
            index >> 10
        );
        assert!(
            transient <= BIG as usize * 12,
            "sized build {transient} B for {BIG} entries"
        );
    }

    #[test]
    fn oversized_hints_fall_back_to_an_unsized_build() {
        let baseline = MftFixture::new(ENTRIES, 7).read().index;
        let memory = baseline.table.memory_bytes() + baseline.ids.memory_bytes();
        // The table hint is the only reservation that can fail; dense pages reserve nothing ahead.
        let hint = SizeHint {
            entries: 100_000_000,
            folders: 0,
            name_bytes: 0,
        };
        RESERVE_FAILURES.with(|count| count.set(0));
        let result = {
            let _failing = fail_allocations_from(64 * MIB);
            enumerate_volume_sized(
                &mut MftFixture::new(ENTRIES, 7),
                'C',
                usize::MAX,
                hint,
                &CancelToken::new(),
            )
            .unwrap()
        };
        assert_eq!(RESERVE_FAILURES.with(std::cell::Cell::get), 1);
        assert_eq!(result.index.table.len(), ENTRIES as usize);
        assert_eq!(result.index.ids.len(), ENTRIES as usize);
        let built = result.index.table.memory_bytes() + result.index.ids.memory_bytes();
        assert!(
            built <= memory + memory / 10,
            "{built} bytes, baseline {memory}"
        );
    }

    #[test]
    fn an_unsized_build_leaves_no_slack_above_a_sixteenth() {
        let result = MftFixture::new(ENTRIES, 7).read();
        assert!(result.index.table.spares_within_slack());
    }
}
