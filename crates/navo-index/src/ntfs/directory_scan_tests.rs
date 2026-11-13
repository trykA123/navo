//! Pass-one traversal tests with reversed deep folders and files preceding their parents.
use super::*;
use crate::ntfs::test_support::{SyntheticRecord, append_record, journal_metadata, stream_chunk};
use crate::usn::JournalPosition;
struct Volume(usize);
impl VolumeIo for Volume {
    /// Returns stable journal metadata for both continuity checks.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        Ok(journal_metadata(7))
    }
    /// Streams 4,500 reversed folder/file pairs in bounded chunks, from the supplied cursor.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        let start = u32::try_from(cursor).unwrap();
        if start >= 9000 {
            return Ok(None);
        }
        let bytes = stream_chunk(start, (start + 512).min(9000), |bytes, at| {
            let number = 4499 - u64::from(at / 2);
            let folder = at % 2 == 1;
            let file_id = if folder { 5 + number } else { 10000 + number };
            let parent_id = 5 + number - u64::from(folder && number > 0);
            let name = if folder { "d" } else { "é🙂" };
            let spec = SyntheticRecord {
                file_id,
                parent_id,
                folder,
                name,
            };
            append_record(bytes, spec);
        });
        Ok(Some(bytes))
    }
    /// Rejects journal replay because pass one only queries metadata and enumerates records.
    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        unreachable!()
    }
    /// Counts unwanted size requests, returning a harmless byte size if called.
    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        self.0 += 1;
        Ok(1)
    }
}
#[test]
fn directory_scan_resolves_reverse_deep_order() {
    let mut volume = Volume(0);
    let scan = scan_directories(&mut volume, 'C', usize::MAX, &CancelToken::new()).unwrap();
    let index = scan.index;
    assert_eq!(index.table.len(), 4500);
    for id in 6..4505 {
        assert!(index.ids[&(id - 1)] < index.ids[&id]);
    }
    let path = index.table.path_with(index.ids[&4004], '\\').unwrap();
    assert_eq!(path.matches('\\').count(), 3999);
    assert_eq!(volume.0, 0);
}
#[test]
fn directory_scan_counts_files_and_utf8_names_without_retaining_files() {
    let mut volume = Volume(0);
    let scan = scan_directories(&mut volume, 'C', usize::MAX, &CancelToken::new()).unwrap();
    let cap = scan.capacity;
    let totals = (cap.entries, cap.folders, cap.name_bytes);
    assert_eq!(totals, (9000, 4500, 40501));
    assert_eq!(scan.index.ids.len(), 4500);
    assert!(!scan.index.ids.contains_key(&10000));
    assert_eq!(scan.index.position.next_usn, 100);
    assert_eq!(volume.0, 0);
}
