//! Exercises hard-link refresh through journal reads and index replay.

use super::journal_churn_tests::wire;
use super::*;
use crate::ntfs::test_support::journal_metadata;
use crate::usn::PendingRenames;

const ROOT: u64 = (5 << 48) | 5;
const FILE: u64 = (1 << 48) | 20;
const LINK_CLOSE: u32 = 0x80010000;
const SAVED: JournalPosition = JournalPosition {
    journal_id: 7,
    next_usn: 100,
};

struct Volume {
    journal: Vec<u8>,
    mft: Result<Option<Vec<u8>>, NtfsError>,
    lookups: usize,
}

impl Volume {
    /// Creates a journal with one link change and the supplied MFT lookup result.
    fn new(mft: Result<Option<Vec<u8>>, NtfsError>) -> Self {
        Self {
            journal: wire(120, &[(FILE, 8, false, "reported.txt", LINK_CLOSE)]),
            mft,
            lookups: 0,
        }
    }
}

impl VolumeIo for Volume {
    /// Reports that the saved cursor is readable and the journal ends at 120.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        let mut bytes = journal_metadata(SAVED.journal_id);
        bytes[16..24].copy_from_slice(&120i64.to_le_bytes());
        Ok(bytes)
    }

    /// Counts lookups at the file's record number and returns the scripted response.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        assert_eq!(cursor, 20);
        self.lookups += 1;
        std::mem::replace(&mut self.mft, Ok(None))
    }

    /// Returns the scripted journal batch.
    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        Ok(self.journal.clone())
    }

    /// Returns the size of the surviving file in bytes.
    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        Ok(10)
    }
}

/// Replays a link change as an upsert from the first named MFT record.
#[test]
fn link_refresh_upserts_the_mft_name_parent_and_flags() {
    let mut mft = wire(21, &[(FILE, 9, false, "survivor.txt", 0)]);
    mft[60..64].copy_from_slice(&2u32.to_le_bytes());
    mft.push(0);
    let mut volume = Volume::new(Ok(Some(mft)));
    let batch = read_volume_journal(&mut volume, SAVED);
    assert!(batch.is_ok(), "live file should refresh: {batch:?}");
    let batch = batch.unwrap();
    assert!(matches!(
        &batch.clone().into_changes(&mut PendingRenames::default())[0],
        TableChange::Add { file_id, parent_id: 9, name, flags }
            if *file_id == u128::from(FILE) && name == "survivor.txt" && *flags == Flags::HIDDEN
    ));

    let mut index = NtfsIndex::new(
        'C',
        JournalPosition {
            next_usn: 90,
            ..SAVED
        },
    )
    .unwrap();
    let original = wire(
        100,
        &[
            (8, ROOT, true, "old", 0x100),
            (9, ROOT, true, "new", 0x100),
            (FILE, 8, false, "original.txt", 0x100),
        ],
    );
    assert!(
        index
            .apply_batch(parse_batch(&original).unwrap())
            .unwrap()
            .is_empty()
    );
    let id = index.ids[&u128::from(FILE)];
    assert!(index.apply_batch(batch).unwrap().is_empty());
    assert_eq!(index.ids[&u128::from(FILE)], id);
    assert_eq!(
        index.table.path_with(id, '\\').as_deref(),
        Some("C:\\new\\survivor.txt")
    );
    assert_eq!(index.table.flags(id), Some(Flags::HIDDEN));
}

/// Covers EOF, an empty response, and a response starting at the next slot.
#[test]
fn link_refresh_removes_a_missing_file() {
    for mft in [
        None,
        Some(wire(21, &[])),
        Some(wire(22, &[(21, 8, false, "next.txt", 0)])),
    ] {
        let batch = read_volume_journal(&mut Volume::new(Ok(mft)), SAVED);
        assert!(batch.is_ok(), "missing file should remove: {batch:?}");
        assert_eq!(
            batch.unwrap().into_changes(&mut PendingRenames::default()),
            [TableChange::Remove {
                file_id: u128::from(FILE)
            }]
        );
    }
}

/// Treats a different sequence number in the same slot as a missing reference.
#[test]
fn link_refresh_removes_the_old_reference_when_its_record_is_reused() {
    let mft = wire(21, &[((2 << 48) | 20, 8, false, "reused.txt", 0)]);
    let batch = read_volume_journal(&mut Volume::new(Ok(Some(mft))), SAVED);
    assert!(batch.is_ok(), "reused reference should remove: {batch:?}");
    assert_eq!(
        batch.unwrap().into_changes(&mut PendingRenames::default()),
        [TableChange::Remove {
            file_id: u128::from(FILE)
        }]
    );
}

/// Looks up a repeated link once and preserves unrelated journal records.
#[test]
fn link_refresh_only_looks_up_link_records_and_deduplicates_by_file() {
    let mft = wire(21, &[(FILE, 9, false, "survivor.txt", 0)]);
    let mut volume = Volume::new(Ok(Some(mft)));
    let attributes = (FILE, 8, false, "reported.txt", 0x8000);
    volume.journal = wire(120, &[attributes]);
    let unchanged = read_volume_journal(&mut volume, SAVED).unwrap();
    assert_eq!(unchanged, parse_batch(&volume.journal).unwrap());
    assert_eq!(volume.lookups, 0);

    let link = (FILE, 8, false, "reported.txt", LINK_CLOSE);
    volume.journal = wire(120, &[link, attributes, link]);
    let batch = read_volume_journal(&mut volume, SAVED);
    assert!(
        batch.is_ok(),
        "repeated link change should refresh: {batch:?}"
    );
    let records = batch.unwrap().records;
    assert_eq!(volume.lookups, 1);
    assert_eq!(records[0].name, "survivor.txt");
    assert_eq!(records[0], records[2]);
    assert_eq!(records[1], unchanged.records[0]);
}

/// Classifies an MFT read failure as hard-link reenumeration.
#[test]
fn link_refresh_reports_lookup_failure_as_hard_link_reenumeration() {
    let error = NtfsError::InvalidResponse("MFT lookup failed");
    let result = read_volume_journal(&mut Volume::new(Err(error)), SAVED);
    assert!(
        matches!(result, Err(NtfsError::Reenumerate("hard link"))),
        "lookup failure should report hard link: {result:?}"
    );
}
