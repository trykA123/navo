//! Pass-two file insertion into the resolved NTFS directory graph, without a blocked queue.

use super::{Flags, NtfsError, NtfsIndex, TableChange, VolumeIo, two_pass_size};
use crate::ntfs::{ROOT_REFERENCE, is_root};
use crate::pipe::CancelToken;

/// Inserts a pass-two file and its size; rejects absent parents or conflicting folder identities.
/// Returns the unknown-size count, skipping vanished files and already resolved directories.
pub(super) fn add_file(
    volume: &mut impl VolumeIo,
    index: &mut NtfsIndex,
    mut record: crate::usn::UsnRecord,
) -> Result<u64, NtfsError> {
    let file_id = record.file_id;
    let folder = is_root(file_id) || is_folder(index, file_id);
    if record.flags.contains(Flags::FOLDER) {
        return folder.then_some(0).ok_or(NtfsError::Reenumerate);
    }
    if folder {
        return Err(NtfsError::Reenumerate);
    }
    let Some(size) = two_pass_size::read_record_size(volume, &record) else {
        return Ok(0);
    };
    if is_root(record.parent_id) {
        record.parent_id = ROOT_REFERENCE;
    }
    if !is_folder(index, record.parent_id) {
        return Err(NtfsError::Reenumerate);
    }
    index.table.apply_in_batch(
        TableChange::Add {
            file_id,
            parent_id: record.parent_id,
            name: record.name,
            flags: record.flags,
        },
        &mut index.ids,
    )?;
    if let Some(bytes) = size.bytes {
        index
            .table
            .apply_in_batch(TableChange::Size { file_id, bytes }, &mut index.ids)?;
    }
    Ok(size.unknown)
}

/// Checks whether a reference belongs to a retained directory in the candidate.
fn is_folder(index: &NtfsIndex, reference: u128) -> bool {
    index
        .ids
        .get(&reference)
        .and_then(|&id| index.table.flags(id))
        .is_some_and(|flags| flags.contains(Flags::FOLDER))
}

/// Rejects cancellation at traversal and journal boundaries.
pub(super) fn check_cancel(cancel: &CancelToken) -> Result<(), NtfsError> {
    if cancel.is_cancelled() {
        return Err(NtfsError::Cancelled);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::ntfs::test_support::{SyntheticRecord, append_record, journal_metadata};
    use crate::pipe::CancelToken;

    const ROOT: u64 = (5 << 48) | 5;
    type Row = (u64, u64, bool, &'static str);

    struct Volume {
        rows: [Vec<Row>; 2],
        pass: usize,
        cursors: Vec<u64>,
        sizes: Vec<(usize, u128)>,
        queries: usize,
        reset_at: usize,
        wrap: bool,
        cancel: CancelToken,
        cancel_query: usize,
        cancel_read: Option<(usize, u64)>,
    }

    impl Volume {
        /// Replays one bounded chunk per traversal, with files preceding their parent folders.
        fn new() -> Self {
            let rows = vec![(20, 8, false, "file"), (8, ROOT, true, "dir")];
            Self {
                rows: [rows.clone(), rows],
                pass: 0,
                cursors: Vec::new(),
                sizes: Vec::new(),
                queries: 0,
                reset_at: 0,
                wrap: false,
                cancel: CancelToken::new(),
                cancel_query: 0,
                cancel_read: None,
            }
        }
    }

    impl VolumeIo for Volume {
        /// Supplies stable metadata, or injects cancellation, reset or wrap at a boundary.
        fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
            self.queries += 1;
            if self.queries == self.cancel_query {
                self.cancel.cancel();
            }
            let mut bytes = journal_metadata(7);
            let offset = if self.wrap { 8 } else { 0 };
            let value: u64 = if self.wrap { 101 } else { 8 };
            if self.queries == self.reset_at {
                bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
                bytes[16..24].copy_from_slice(&120i64.to_le_bytes());
            }
            Ok(bytes)
        }

        /// Encodes the current pass independently and returns EOF on its next cursor.
        fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
            if cursor == 0 {
                self.pass += 1;
            }
            self.cursors.push(cursor);
            if self.cancel_read == Some((self.pass, cursor)) {
                self.cancel.cancel();
            }
            if cursor != 0 {
                return Ok(None);
            }
            let mut bytes = 1u64.to_le_bytes().to_vec();
            for &(file_id, parent_id, folder, name) in &self.rows[self.pass - 1] {
                append_record(
                    &mut bytes,
                    SyntheticRecord {
                        file_id,
                        parent_id,
                        folder,
                        name,
                    },
                );
            }
            Ok(Some(bytes))
        }

        /// Full builds retain their original cursor without replaying the journal themselves.
        fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
            unreachable!("full builds do not catch up")
        }

        /// Records which traversal requested each file size, in bytes.
        fn file_size(&mut self, file_id: u128) -> Result<u64, NtfsError> {
            self.sizes.push((self.pass, file_id));
            Ok(10)
        }
    }

    #[test]
    fn two_pass_streams_files_once_into_resolved_parents_without_blocking() {
        let mut volume = Volume::new();
        let built = enumerate_volume(&mut volume, 'C', usize::MAX, &CancelToken::new()).unwrap();
        assert_eq!(volume.cursors, [0, 1, 0, 1]);
        assert_eq!(volume.sizes, [(2, 20)]);
        assert_eq!(built.blocked_peak, 0);
        assert!(built.unresolved.is_empty());
        assert_eq!(
            built.index.position,
            JournalPosition {
                journal_id: 7,
                next_usn: 100
            }
        );
        assert_eq!(
            built
                .index
                .table
                .path_with(built.index.ids[&20], '\\')
                .as_deref(),
            Some("C:\\dir\\file")
        );
        let totals = built.index.table.totals(0).unwrap();
        assert_eq!((totals.bytes, totals.files, totals.folders), (10, 1, 1));
    }

    #[test]
    fn two_pass_uses_one_logical_record_budget() {
        for limit in 0..=4 {
            let mut volume = Volume::new();
            let rows = vec![
                (8, ROOT, true, "dir"),
                (20, 8, false, "file"),
                (21, 8, false, "last"),
            ];
            volume.rows = [rows.clone(), rows];
            let built = enumerate_volume(&mut volume, 'C', limit, &CancelToken::new()).unwrap();
            assert_eq!(volume.pass, if limit == 0 { 0 } else { 2 }, "limit {limit}");
            assert_eq!(built.index.table.len(), 1 + limit.min(3));
            assert_eq!(volume.sizes.len(), limit.min(3).saturating_sub(1));
            assert!(volume.sizes.iter().all(|&(pass, _)| pass == 2));
        }
    }

    #[test]
    fn two_pass_discards_cancellation_during_reads_and_after_boundary_queries() {
        for (query, read) in [(2, None), (3, None), (0, Some((2, 0))), (0, Some((2, 1)))] {
            let mut volume = Volume::new();
            volume.cancel_query = query;
            volume.cancel_read = read;
            let cancel = volume.cancel.clone();
            let result = enumerate_volume(&mut volume, 'C', usize::MAX, &cancel);
            assert!(
                matches!(result, Err(NtfsError::Cancelled)),
                "query {query}, read {read:?}"
            );
        }
    }

    #[test]
    fn two_pass_rejects_journal_reset_or_wrap_at_either_boundary() {
        for query in [2, 3] {
            for wrap in [false, true] {
                let mut volume = Volume::new();
                volume.reset_at = query;
                volume.wrap = wrap;
                let result = enumerate_volume(&mut volume, 'C', usize::MAX, &CancelToken::new());
                assert!(
                    matches!(result, Err(NtfsError::Reenumerate)),
                    "query {query}, wrap {wrap}"
                );
            }
        }
    }

    #[test]
    fn two_pass_rejects_files_whose_parent_was_absent_from_pass_one() {
        let mut volume = Volume::new();
        volume.rows[1] = vec![(20, 9, false, "file"), (9, ROOT, true, "new")];
        let result = enumerate_volume(&mut volume, 'C', usize::MAX, &CancelToken::new());
        assert!(matches!(result, Err(NtfsError::Reenumerate)));
    }
}
