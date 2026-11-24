//! Pass-two record classification and size-aware NTFS file insertion.

use super::{
    Flags, NtfsError, NtfsIndex, TableChange, VolumeIo, reconcile::LateRecords, two_pass_size,
};
use crate::ntfs::{ROOT_REFERENCE, is_folder, is_root};
use crate::pipe::CancelToken;
use crate::usn::UsnRecord;

/// Inserts a file under a known folder or holds the record for reconciliation.
pub(super) fn add_record(
    volume: &mut impl VolumeIo,
    index: &mut NtfsIndex,
    mut record: UsnRecord,
    late: &mut LateRecords,
) -> Result<u64, NtfsError> {
    let file_id = record.file_id;
    let folder = is_root(file_id) || is_folder(index, file_id);
    if record.flags.contains(Flags::FOLDER) {
        if !folder {
            late.hold_folder(record.into());
        }
        return Ok(0);
    }
    if folder {
        late.skip();
        return Ok(0);
    }
    if is_root(record.parent_id) {
        record.parent_id = ROOT_REFERENCE;
    }
    if !is_folder(index, record.parent_id) {
        late.hold_file(record);
        return Ok(0);
    }
    Ok(add_file(volume, index, record)?.unwrap_or(0))
}

/// Inserts a file under its indexed parent and returns the unknown-size count.
/// Returns `None` if the file vanished before its size could be read.
pub(super) fn add_file(
    volume: &mut impl VolumeIo,
    index: &mut NtfsIndex,
    record: UsnRecord,
) -> Result<Option<u64>, NtfsError> {
    let Some(size) = two_pass_size::read_record_size(volume, &record) else {
        return Ok(None);
    };
    let file_id = record.file_id;
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
    Ok(Some(size.unknown))
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

        /// Runs both scripted passes to completion or returns a build error.
        fn build(&mut self) -> Result<Enumeration, NtfsError> {
            enumerate_volume(self, 'C', usize::MAX, &CancelToken::new())
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
        for (query, cause) in [(2, "pass1_continuity"), (3, "pass2_continuity")] {
            for wrap in [false, true] {
                let mut volume = Volume::new();
                volume.reset_at = query;
                volume.wrap = wrap;
                let result = enumerate_volume(&mut volume, 'C', usize::MAX, &CancelToken::new());
                assert!(
                    matches!(&result, Err(NtfsError::Reenumerate(_))),
                    "query {query}, wrap {wrap}"
                );
                assert!(result.unwrap_err().to_string().contains(cause), "{cause}");
            }
        }
    }

    /// Keeps a build the journal outran at either boundary, resuming at the journal's end (AF12).
    #[test]
    fn allow_wrap_keeps_a_wrapped_build_as_journal_wrapped_at_the_latest_cursor() {
        for (query, wrap) in [(2, false), (2, true), (3, false), (3, true)] {
            let mut volume = Volume::new();
            volume.reset_at = query;
            volume.wrap = wrap;
            let built = enumerate_volume_with_continuity(
                &mut volume,
                'C',
                SizeHint::default(),
                BuildPass {
                    continuity: Continuity::AllowWrap,
                    cancel: &CancelToken::new(),
                },
            )
            .unwrap_or_else(|error| panic!("query {query}, wrap {wrap}: {error}"));
            let latest = match (query, wrap) {
                (3, false) => (8, 120),
                (3, true) => (7, 120),
                _ => (7, 100),
            };
            let position = built.index.position;
            assert_eq!((position.journal_id, position.next_usn), latest);
            assert_eq!(
                built.inexact_reason(),
                Some(crate::ntfs::InexactBuild::JournalWrapped)
            );
            assert!(built.inexact, "query {query}, wrap {wrap}");
            assert_eq!(built.index.table.len(), 3);
        }
    }

    /// Leaves an unbroken journal exact under `AllowWrap`, at the build's start cursor.
    #[test]
    fn allow_wrap_on_an_unbroken_journal_stays_exact() {
        let mut volume = Volume::new();
        let built = enumerate_volume_with_continuity(
            &mut volume,
            'C',
            SizeHint::default(),
            BuildPass {
                continuity: Continuity::AllowWrap,
                cancel: &CancelToken::new(),
            },
        )
        .unwrap();
        assert_eq!(built.inexact_reason(), None);
        assert!(!built.inexact);
        assert_eq!(built.index.position.next_usn, 100);
    }

    /// Keeps the stable rows and the subtree reported by pass two.
    #[test]
    fn a_folder_created_between_passes_is_reconciled() {
        let mut volume = Volume::new();
        volume.rows[1].extend([(9, ROOT, true, "new"), (21, 9, false, "new-file")]);
        let built = volume.build().unwrap();
        assert_eq!(built.skipped, 0);
        assert_eq!(built.index.table.len(), 5);
        assert!(built.index.ids.contains_key(&8));
        assert!(built.index.ids.contains_key(&20));
        assert!(built.index.ids.contains_key(&21));
    }

    /// Counts a plain orphan folder and file whose parent never appears in either pass.
    #[test]
    fn a_plain_orphan_keeps_the_build_exact() {
        let mut volume = Volume::new();
        for rows in &mut volume.rows {
            rows.extend([
                (21, 9, false, "orphan-file"),
                (22, 9, true, "orphan-folder"),
            ]);
        }
        let built = volume.build().unwrap();
        assert!(!built.inexact, "plain orphans must keep the build exact");
        assert_eq!(built.skipped, 2);
        assert!(!built.index.ids.contains_key(&21));
        assert!(!built.index.ids.contains_key(&22));
    }

    /// Keeps the pass-one folder when pass two reports a file with its ID.
    #[test]
    fn a_reused_folder_id_as_a_file_is_skipped() {
        let mut volume = Volume::new();
        volume.rows[1].push((8, ROOT, false, "reused"));
        let built = volume.build().unwrap();
        assert_eq!(built.skipped, 1);
        assert_eq!(built.index.table.name(built.index.ids[&8]), Some("dir"));
    }

    /// Inserts a file held before its pass-two parent arrives.
    #[test]
    fn an_orphan_folder_found_in_pass_two_adopts_its_held_files() {
        let mut volume = Volume::new();
        volume.rows[1] = vec![(20, 9, false, "file"), (9, ROOT, true, "new")];
        let built = volume.build().unwrap();
        assert_eq!(built.skipped, 0);
        let index = built.index;
        assert_eq!(index.table.parent(index.ids[&20]), Some(index.ids[&9]));
        assert_eq!(index.table.size(index.ids[&20]), Some(10));
    }

    /// Replaces a cyclic pass-one parent with the newer parent and keeps its descendants.
    #[test]
    fn a_folder_moved_during_pass_one_keeps_its_subtree() {
        let mut volume = Volume::new();
        let b = (9, 8, true, "B");
        let f = (20, 9, false, "F");
        volume.rows = [
            vec![(8, 9, true, "A"), b, f],
            vec![f, b, (8, ROOT, true, "A")],
        ];
        let built = volume.build().unwrap();
        assert_eq!(built.skipped, 0);
        let index = built.index;
        assert_eq!(
            index.table.path_with(index.ids[&20], '\\').as_deref(),
            Some("C:\\A\\B\\F")
        );
        assert_eq!(index.table.len(), 4);
    }

    /// Replays the same inconsistent parent reads during both passes of the reviewer scenario.
    #[test]
    fn the_reviewers_double_cycle_marks_the_build_inexact() {
        let mut volume = Volume::new();
        let rows = vec![(8, 9, true, "A"), (9, 8, true, "B"), (20, 9, false, "F")];
        volume.rows = [rows.clone(), rows];
        let built = volume.build().unwrap();
        assert!(built.inexact, "both scans observed the cycle");
        assert_eq!(built.skipped, 3);
        assert_eq!(built.index.table.len(), 1);
    }

    /// Holds only the first file when both precede their late parent and the cap is one.
    #[test]
    fn held_files_over_the_cap_mark_the_build_inexact() {
        let mut volume = Volume::new();
        volume.rows[1].extend([
            (21, 9, false, "held"),
            (22, 9, false, "skipped"),
            (9, ROOT, true, "late"),
        ]);
        let built = super::super::enumerate_with_late_limits::<1, 1_000_000>(
            &mut volume,
            'C',
            usize::MAX,
            SizeHint::default(),
            Continuity::Strict,
            &CancelToken::new(),
        )
        .unwrap();
        assert!(built.inexact, "a held file was dropped by the cap");
        assert_eq!(built.skipped, 1);
        assert!(built.index.ids.contains_key(&21));
        assert!(!built.index.ids.contains_key(&22));
    }

    /// Counts a new FRN beyond the folder cap while allowing a retained FRN to be refreshed.
    #[test]
    fn late_folders_over_the_cap_mark_the_build_inexact() {
        let mut volume = Volume::new();
        volume.rows[0].push((9, 40, true, "old"));
        volume.rows[1].extend([(10, ROOT, true, "dropped"), (9, ROOT, true, "placed")]);
        let built = super::super::enumerate_with_late_limits::<1_000_000, 1>(
            &mut volume,
            'C',
            usize::MAX,
            SizeHint::default(),
            Continuity::Strict,
            &CancelToken::new(),
        )
        .unwrap();
        assert!(built.inexact, "a late folder was dropped by the cap");
        assert_eq!(built.skipped, 1);
        assert_eq!(built.index.table.name(built.index.ids[&9]), Some("placed"));
        assert!(!built.index.ids.contains_key(&10));
    }
}
