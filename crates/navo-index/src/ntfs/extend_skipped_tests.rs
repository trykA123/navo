//! AF9f: a full build leaves `$Deleted` in the skipped set; the `$Extend` walk must still run for it.

use super::super::test_support::{SyntheticRecord, append_record, extend_folder_build};
use super::extend_chain::metadata_parents;
use super::{JournalPosition, NtfsError, NtfsIndex, VolumeIo};
use crate::table::Flags;
use crate::usn::{UsnBatch, UsnRecord};

const EXTEND: u64 = (11 << 48) | 11;
const DELETED: u64 = (1 << 48) | 0x1D;
const FILE: u128 = 21;

/// An MFT holding only `$Deleted` (record 29, parent `$Extend`).
struct DeletedMft;

impl VolumeIo for DeletedMft {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        unreachable!("the walk reads MFT records only")
    }

    /// Returns the `$Deleted` record for a read at its slot, else the end of the volume.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        if cursor > 0x1D {
            return Ok(None);
        }
        let mut bytes = (cursor + 1).to_le_bytes().to_vec();
        let spec = SyntheticRecord {
            file_id: DELETED,
            parent_id: EXTEND,
            folder: true,
            name: "$Deleted",
        };
        append_record(&mut bytes, spec);
        Ok(Some(bytes))
    }

    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        unreachable!("the walk reads MFT records only")
    }

    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        unreachable!("the walk reads MFT records only")
    }
}

/// VM pattern after a full build: `$Deleted` is in `skipped_folders` but not in
/// `metadata_folders`; a rename into it must still be recognised through the MFT walk.
#[test]
fn a_parent_in_the_skipped_set_is_still_walked_to_extend() {
    let mut index = extend_folder_build().index;
    index.metadata_folders.clear();
    index.skipped_folders.insert(u128::from(DELETED));
    let record = UsnRecord {
        file_id: FILE,
        parent_id: u128::from(DELETED),
        usn: 110,
        reason: 0x2000,
        flags: Flags::NONE,
        name: "$X15".into(),
        size: Some(0),
    };
    let batch = UsnBatch {
        next_usn: 120,
        records: vec![record],
    };
    let found = metadata_parents(&mut DeletedMft, &batch, &index);
    assert_eq!(found, vec![u128::from(DELETED)]);
}

/// Builds one journal record of `FILE` with the given parent, reason, name and size.
fn file_record(parent_id: u128, reason: u32, name: &str, size: Option<u64>) -> UsnRecord {
    UsnRecord {
        file_id: FILE,
        parent_id,
        usn: 110,
        reason,
        flags: Flags::NONE,
        name: name.into(),
        size,
    }
}

/// The VM pattern: the indexed `x.txt` is renamed into `$Deleted` (paired with its old name or
/// not), then shrinks there.
fn parked_with_size(paired: bool) -> UsnBatch {
    let mut records = vec![
        file_record(u128::from(DELETED), 0x2000, "$X15", None),
        file_record(u128::from(DELETED), 0x2, "$X15", Some(0)),
    ];
    if paired {
        records.insert(0, file_record(30, 0x1000, "x.txt", None));
    }
    UsnBatch {
        next_usn: 120,
        records,
    }
}

/// Reads the indexed name of `FILE`, or `None` once it left the index.
fn indexed_file(index: &NtfsIndex) -> Option<&str> {
    index.ids.get(&FILE).and_then(|&id| index.table.name(id))
}

/// AF9g rule: a paired rename of an indexed file into a metadata folder is its delete; the size
/// record that follows it there is dropped, so the batch reports nothing and asks for no rebuild.
/// AF9h: an unpaired one may be another hard link leaving, so it stays blocked and indexed.
#[test]
fn a_rename_into_a_metadata_folder_drops_its_size_record() {
    for paired in [true, false] {
        let mut index = extend_folder_build().index;
        assert_eq!(indexed_file(&index), Some("x.txt"));
        let unresolved = index.apply_batch(parked_with_size(paired)).expect("apply");
        if paired {
            assert_eq!(unresolved, []);
            assert_eq!(indexed_file(&index), None);
        } else {
            assert_ne!(unresolved, [], "the unpaired change stays blocked");
            assert_eq!(indexed_file(&index), Some("x.txt"));
        }
    }
}

/// AF9h: file F has links A (`x.txt`, indexed) and B (`b.txt`). B's old name ends one read and
/// its new name in `$Deleted` starts the next, so the batch cannot tell which link left: F stays
/// indexed under A and the change is reported blocked, not applied.
#[test]
fn a_split_rename_of_another_link_into_deleted_keeps_the_file() {
    let mut index = extend_folder_build().index;
    let old_read = UsnBatch {
        next_usn: 115,
        records: vec![file_record(30, 0x1000, "b.txt", None)],
    };
    assert_eq!(index.apply_batch(old_read).expect("first read"), []);
    let new_read = UsnBatch {
        next_usn: 120,
        records: vec![file_record(u128::from(DELETED), 0x2000, "$X15", None)],
    };
    let unresolved = index.apply_batch(new_read).expect("second read");
    assert_ne!(unresolved, [], "the unpaired new name stays blocked");
    assert_eq!(indexed_file(&index), Some("x.txt"));
}

/// AF9g end to end: `$Deleted` sits in the skipped set only; the walk confirms it under
/// `$Extend`, so the paired parked file leaves the index with no unresolved change. AF9h: the
/// unpaired one stays blocked and indexed, as in a metadata folder the index knew.
#[test]
fn a_rename_into_a_skipped_only_deleted_folder_needs_no_rebuild() {
    for paired in [true, false] {
        let mut index = extend_folder_build().index;
        index.metadata_folders.clear();
        index.skipped_folders.insert(u128::from(DELETED));
        let batch = parked_with_size(paired);
        let found = metadata_parents(&mut DeletedMft, &batch, &index);
        assert_eq!(found, vec![u128::from(DELETED)], "paired={paired}");
        index.note_metadata_folders(found);
        let unresolved = index.apply_batch(batch).expect("apply");
        if paired {
            assert_eq!(unresolved, []);
            assert_eq!(indexed_file(&index), None);
        } else {
            assert_ne!(unresolved, [], "the unpaired change stays blocked");
            assert_eq!(indexed_file(&index), Some("x.txt"));
        }
    }
}

/// An MFT whose walk finds nothing, but whose by-id path query answers with a fixed path.
struct PathOnly(&'static str);

impl VolumeIo for PathOnly {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        unreachable!("the check reads MFT records and paths only")
    }

    /// Ends the volume at once, so the walk cannot place any parent.
    fn enumerate(&mut self, _: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        Ok(None)
    }

    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        unreachable!("the check reads MFT records and paths only")
    }

    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        unreachable!("the check reads MFT records and paths only")
    }

    fn path_by_id(&mut self, _: u128) -> Result<Option<Vec<u16>>, NtfsError> {
        Ok(Some(self.0.encode_utf16().collect()))
    }
}

/// AF9g fallback: a parent the walk cannot place is metadata when its by-id path starts with
/// `\$Extend\` in any letter case, and not when `$Extend` is deeper or only a name prefix.
#[test]
fn a_parent_the_walk_misses_is_metadata_by_its_extend_path() {
    let mut index = extend_folder_build().index;
    index.metadata_folders.clear();
    let batch = parked_with_size(false);
    let cases = [
        (r"\$EXTEND\$Deleted", vec![u128::from(DELETED)]),
        (r"\Users\$Extend\$Deleted", vec![]),
        (r"\$ExtendX\$Deleted", vec![]),
        (r"\$Extend", vec![]),
    ];
    for (path, expected) in cases {
        let found = metadata_parents(&mut PathOnly(path), &batch, &index);
        assert_eq!(found, expected, "{path}");
    }
}

/// AF9g diagnostic: a missing reference is named metadata (also when skipped too), skipped or
/// unknown.
#[test]
fn a_missing_reference_is_classed_by_what_the_index_knows() {
    use super::super::MissingClass;
    let mut index = extend_folder_build().index;
    index.metadata_folders.clear();
    index.skipped_folders.insert(u128::from(DELETED));
    assert_eq!(
        index.missing_class(u128::from(DELETED)),
        MissingClass::Skipped
    );
    index.note_metadata_folders([u128::from(DELETED)]);
    assert_eq!(
        index.missing_class(u128::from(DELETED)),
        MissingClass::Metadata
    );
    assert_eq!(
        index.missing_class(u128::from(EXTEND)),
        MissingClass::Metadata
    );
    assert_eq!(index.missing_class(777), MissingClass::Unknown);
}

/// Counts MFT reads and path queries; no record is readable, and every path is in `$Deleted`.
#[derive(Debug, Default)]
struct CountingMft {
    reads: usize,
    queries: usize,
}

impl VolumeIo for CountingMft {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        unreachable!("the check reads MFT records and paths only")
    }

    /// Counts the read and ends the volume, so the walk stops without an answer.
    fn enumerate(&mut self, _: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        self.reads += 1;
        Ok(None)
    }

    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        unreachable!("the check reads MFT records and paths only")
    }

    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        unreachable!("the check reads MFT records and paths only")
    }

    /// Counts the query and answers with the `$Deleted` path.
    fn path_by_id(&mut self, _: u128) -> Result<Option<Vec<u16>>, NtfsError> {
        self.queries += 1;
        Ok(Some(r"\$Extend\$Deleted".encode_utf16().collect()))
    }
}

/// AF9h cost bound: 1,000 data records under a skipped-only folder cost no read and no query;
/// one new name under it costs at most one walk read and one path query.
#[test]
fn data_records_under_a_skipped_folder_cost_no_walk_or_query() {
    let mut index = extend_folder_build().index;
    index.metadata_folders.clear();
    index.skipped_folders.insert(u128::from(DELETED));
    let data = file_record(u128::from(DELETED), 0x2, "$X15", Some(0));
    let mut batch = UsnBatch {
        next_usn: 120,
        records: vec![data; 1_000],
    };
    let mut volume = CountingMft::default();
    assert!(metadata_parents(&mut volume, &batch, &index).is_empty());
    assert_eq!((volume.reads, volume.queries), (0, 0));
    batch
        .records
        .push(file_record(u128::from(DELETED), 0x2000, "$X15", None));
    let found = metadata_parents(&mut volume, &batch, &index);
    assert_eq!(found, vec![u128::from(DELETED)]);
    assert!(volume.reads <= 1 && volume.queries <= 1, "{volume:?}");
}
