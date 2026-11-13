//! Pass-one parent, record-budget, cancellation and journal continuity edge cases.
use super::*;
use crate::ntfs::test_support::{SyntheticRecord, append_record, journal_metadata};
use crate::usn::JournalPosition;
#[derive(Default)]
struct Volume {
    chunks: Vec<Vec<u8>>,
    end: Vec<u8>,
    cancel: CancelToken,
    cancel_at: usize,
    calls: usize,
}
impl Volume {
    /// Encodes one record per chunk; names use two bytes including the terminating NUL.
    fn new(records: &[(u64, u64, bool)]) -> Self {
        let mut volume = Self::default();
        for &(file_id, parent_id, folder) in records {
            let next = u64::try_from(volume.chunks.len() + 1).unwrap();
            let mut bytes = next.to_le_bytes().to_vec();
            let spec = SyntheticRecord {
                file_id,
                parent_id,
                folder,
                name: "x",
            };
            append_record(&mut bytes, spec);
            volume.chunks.push(bytes);
        }
        volume
    }
    /// Cancels at the selected I/O call, allowing deterministic checkpoint coverage.
    fn step(&mut self) {
        self.calls += 1;
        if self.calls == self.cancel_at {
            self.cancel.cancel();
        }
    }
}
impl VolumeIo for Volume {
    /// Returns the initial cursor, then the scripted final journal state.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        self.step();
        Ok(if self.end.is_empty() || self.calls == 1 {
            journal_metadata(7)
        } else {
            self.end.clone()
        })
    }
    /// Replays chunks by cursor and returns EOF after the last chunk.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        self.step();
        Ok(self.chunks.get(usize::try_from(cursor).unwrap()).cloned())
    }
    /// Rejects unexpected replay during pass one.
    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        unreachable!("pass one must not replay the journal")
    }
    /// Rejects size I/O because pass one only retains folders and scalar counts.
    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        unreachable!("pass one must not read file sizes")
    }
}
#[test]
fn directory_scan_resolves_later_parents_and_sequenced_roots() {
    let root = (5 << 48) | 5;
    let records = [(10, 20, true), (20, root, true), (root, root, true)];
    let mut volume = Volume::new(&records);
    let scan = scan_directories(&mut volume, 'C', usize::MAX, &CancelToken::new()).unwrap();
    let index = scan.index;
    let path = index.table.path_with(index.ids[&10], '\\');
    assert_eq!(path.as_deref(), Some("C:\\x\\x"));
    assert_eq!(index.ids[&u128::from(root)], index.ids[&5]);
    assert!(index.ids[&20] < index.ids[&10]);
}
#[test]
fn directory_scan_rejects_missing_and_cyclic_parents() {
    for records in [
        &[(10, 999, true)][..],
        &[(10, 10, true)],
        &[(10, 20, true), (20, 10, true)],
    ] {
        let mut volume = Volume::new(records);
        let result = scan_directories(&mut volume, 'C', usize::MAX, &CancelToken::new());
        assert!(matches!(result, Err(NtfsError::Reenumerate)));
    }
}
#[test]
fn directory_scan_limits_count_folders_and_files_including_exact_eof() {
    for limit in 0..=4 {
        let records = [(10, 5, true), (11, 10, false), (12, 5, true)];
        let mut volume = Volume::new(&records);
        let mut chunk = 1u64.to_le_bytes().to_vec();
        for record in &volume.chunks {
            chunk.extend_from_slice(&record[8..]);
        }
        volume.chunks = vec![chunk];
        let scan = scan_directories(&mut volume, 'C', limit, &CancelToken::new()).unwrap();
        let count = limit.min(3);
        let folders = 1 + usize::from(count >= 1) + usize::from(count >= 3);
        assert_eq!(
            scan.capacity,
            SizeHint {
                entries: count + 1,
                folders,
                name_bytes: 3 + 2 * count
            }
        );
        assert_eq!(scan.index.table.len(), folders);
        assert_eq!(scan.index.ids.len(), folders);
        assert_eq!(scan.index.ids.contains_key(&12), count >= 3);
        assert!(!scan.index.ids.contains_key(&11));
    }
}
#[test]
fn directory_scan_cancellation_is_checked_before_batches_after_limit_or_eof_and_after_query() {
    for limit in [1, usize::MAX] {
        let last = if limit == 1 { 3 } else { 5 };
        for at in 0..=last {
            let mut volume = Volume::new(&[(10, 5, true), (20, 10, true)]);
            volume.cancel_at = at;
            let cancel = volume.cancel.clone();
            if at == 0 {
                cancel.cancel();
            }
            let result = scan_directories(&mut volume, 'C', limit, &cancel);
            assert!(matches!(result, Err(NtfsError::Cancelled)), "I/O call {at}");
            assert_eq!(volume.calls, at.max(1));
        }
    }
}
#[test]
fn directory_scan_reenumerates_if_the_journal_is_recreated_or_leaves_its_readable_range() {
    for (offset, value) in [(0, 8u64), (8, 101), (24, 101)] {
        let mut volume = Volume::new(&[(10, 5, true)]);
        volume.end = journal_metadata(7);
        volume.end[16..24].copy_from_slice(&120i64.to_le_bytes());
        volume.end[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        let result = scan_directories(&mut volume, 'C', usize::MAX, &CancelToken::new());
        assert!(matches!(result, Err(NtfsError::Reenumerate)));
        assert_eq!(volume.calls, 4);
    }
}
