//! AF9e: after a restore the index has no metadata-folder set; a journal read walks an unknown
//! parent's MFT chain and recognises `$Extend\$Deleted`, so a rename into it needs no rebuild.

use super::super::test_support::{SyntheticRecord, append_record};
use super::extend_chain::{MAX_CHAIN, metadata_parents};
use super::{JournalPosition, NtfsError, NtfsIndex, VolumeIo};
use crate::ntfs::UnresolvedChange;
use crate::table::{Flags, TableChange};
use crate::usn::{UsnBatch, UsnRecord};

const EXTEND: u128 = (11 << 48) | 11;
const DELETED: u128 = (1 << 48) | 0x1D;
const RM: u128 = (1 << 48) | 27;
const TXF: u128 = (1 << 48) | 28;
const DOCS: u128 = 30;
const FILE: u128 = 21;

/// An MFT with `$Deleted`, `$RmMetadata\$Txf` and `docs`, read one record at a time.
struct ChainMft {
    rows: Vec<(u64, u64)>,
    reads: Vec<u64>,
    fail: bool,
}

impl ChainMft {
    fn new() -> Self {
        let rows = [(DELETED, EXTEND), (RM, EXTEND), (TXF, RM), (DOCS, 5)];
        Self {
            rows: rows
                .map(|(file, parent)| (file as u64, parent as u64))
                .into(),
            reads: Vec::new(),
            fail: false,
        }
    }
}

impl VolumeIo for ChainMft {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        unreachable!("the walk reads MFT records only")
    }

    /// Returns the first row at or after the record slot, as `FSCTL_ENUM_USN_DATA` does.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        self.reads.push(cursor);
        if self.fail {
            return Err(NtfsError::InvalidResponse("scripted MFT read failure"));
        }
        let mut rows = self.rows.clone();
        rows.sort_by_key(|&(file, _)| file & 0x0000_ffff_ffff_ffff);
        let Some(&(file_id, parent_id)) = rows
            .iter()
            .find(|&&(file, _)| file & 0x0000_ffff_ffff_ffff >= cursor)
        else {
            return Ok(None);
        };
        let mut bytes = (cursor + 1).to_le_bytes().to_vec();
        let spec = SyntheticRecord {
            file_id,
            parent_id,
            folder: true,
            name: "row",
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

fn record(file_id: u128, parent_id: u128, reason: u32, name: &str) -> UsnRecord {
    UsnRecord {
        file_id,
        parent_id,
        usn: 110,
        reason,
        flags: Flags::NONE,
        name: name.into(),
        size: Some(0),
    }
}

/// The VM pattern after a restart: an open file's last link is deleted, Windows moves it into
/// `$Deleted` and it shrinks there.
fn parked_file() -> UsnBatch {
    UsnBatch {
        next_usn: 120,
        records: vec![
            record(FILE, DOCS, 0x1000, "x.txt"),
            record(FILE, DELETED, 0x2000, "$X15"),
            record(FILE, DELETED, 0x2, "$X15"),
        ],
    }
}

/// Picks a named change: only those ask the service for a rebuild (sizes and attributes drop).
fn named(item: &UnresolvedChange) -> bool {
    matches!(
        item.change,
        TableChange::Add { .. } | TableChange::Rename { .. }
    )
}

/// A restored index: the build's tables without its metadata-folder and skipped sets.
fn restored() -> NtfsIndex {
    let mut index = super::super::test_support::extend_folder_build().index;
    index.metadata_folders.clear();
    index.skipped_folders.clear();
    index
}

/// Without the walk the batch blocks (today's restart rebuild); with it, `$Deleted` is read once
/// and the parked file leaves the index with no unresolved change.
#[test]
fn after_a_restore_a_rename_into_deleted_needs_no_rebuild() {
    let mut blocked = restored();
    assert!(
        blocked
            .apply_batch(parked_file())
            .unwrap()
            .iter()
            .any(named)
    );

    let mut index = restored();
    let mut volume = ChainMft::new();
    let found = metadata_parents(&mut volume, &parked_file(), &index);
    assert_eq!(found, vec![DELETED]);
    assert_eq!(volume.reads, vec![0x1D], "one read; DOCS is indexed");
    index.note_metadata_folders(found);
    let unresolved = index.apply_batch(parked_file()).unwrap();
    assert!(!unresolved.iter().any(named), "{unresolved:?}");
    assert!(index.ids.get(&FILE).is_none());
}

/// A nested metadata folder is found through its parent; an unknown parent whose record slot
/// holds a newer sequence number stays unconfirmed and is read only once.
#[test]
fn a_nested_folder_is_found_and_a_reused_record_is_not() {
    let index = restored();
    let reused = (2 << 48) | 0x1D;
    let batch = UsnBatch {
        next_usn: 120,
        records: vec![
            record(60, TXF, 0x100, "log.blf"),
            record(61, reused, 0x100, "a"),
            record(62, reused, 0x100, "b"),
        ],
    };
    let mut volume = ChainMft::new();
    assert_eq!(metadata_parents(&mut volume, &batch, &index), vec![TXF]);
    assert_eq!(volume.reads, vec![28, 27, 0x1D]);
}

/// A read error stops the walk and confirms nothing, so the change stays blocked (fails closed).
#[test]
fn a_read_error_leaves_the_change_blocked() {
    let mut index = restored();
    let mut volume = ChainMft::new();
    volume.fail = true;
    let found = metadata_parents(&mut volume, &parked_file(), &index);
    assert!(found.is_empty());
    index.note_metadata_folders(found);
    let unresolved = index.apply_batch(parked_file()).unwrap();
    assert!(unresolved.iter().any(named), "{unresolved:?}");
}

/// Shared unknown ancestors `A1`..`A6` (sequence 1), `A1` the lowest.
const ANCESTORS: [u128; 6] = [
    (1 << 48) | 1001,
    (1 << 48) | 1002,
    (1 << 48) | 1003,
    (1 << 48) | 1004,
    (1 << 48) | 1005,
    (1 << 48) | 1006,
];
const PARENTS: u128 = 3_000;

/// An MFT keyed by exact record number that counts reads and can fail one record's read.
struct CountingMft {
    rows: std::collections::HashMap<u64, (u64, u64)>,
    reads: usize,
    fail_at: Option<u64>,
}

impl CountingMft {
    /// `PARENTS` folders below `A1`, the chain `A1`..`A6`, and `A6` below `top`.
    fn shared_chain(top: u128) -> Self {
        let mut rows = std::collections::HashMap::new();
        let mut put = |file: u128, parent: u128| {
            rows.insert(
                file as u64 & 0x0000_ffff_ffff_ffff,
                (file as u64, parent as u64),
            );
        };
        for pair in ANCESTORS.windows(2) {
            put(pair[0], pair[1]);
        }
        put(ANCESTORS[5], top);
        for step in 0..PARENTS {
            put(parent(step), ANCESTORS[0]);
        }
        Self {
            rows,
            reads: 0,
            fail_at: None,
        }
    }
}

impl VolumeIo for CountingMft {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        unreachable!("the walk reads MFT records only")
    }

    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        self.reads += 1;
        if self.fail_at == Some(cursor) {
            return Err(NtfsError::InvalidResponse("scripted MFT read failure"));
        }
        let Some(&(file_id, parent_id)) = self.rows.get(&cursor) else {
            return Ok(None);
        };
        let mut bytes = (cursor + 1).to_le_bytes().to_vec();
        let spec = SyntheticRecord {
            file_id,
            parent_id,
            folder: true,
            name: "row",
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

/// The reference of the `step`th distinct folder below `A1`.
fn parent(step: u128) -> u128 {
    (1 << 48) | (2_000 + step)
}

/// One size change of a file in each of the `PARENTS` folders.
fn spread_batch() -> UsnBatch {
    UsnBatch {
        next_usn: 120,
        records: (0..PARENTS)
            .map(|step| record(100_000 + step, parent(step), 0x2, "f"))
            .collect(),
    }
}

/// 3,000 folders below one unknown 6-level chain under `$Extend`: each folder and each ancestor
/// is read once, and every folder is confirmed.
#[test]
fn a_shared_chain_under_extend_is_read_once_per_batch() {
    let mut volume = CountingMft::shared_chain(EXTEND);
    let found = metadata_parents(&mut volume, &spread_batch(), &restored());
    assert_eq!(found.len(), PARENTS as usize);
    assert_eq!(volume.reads, PARENTS as usize + ANCESTORS.len());
}

/// The same chain below the root: nothing is confirmed, with the same read bound.
#[test]
fn a_shared_chain_under_the_root_is_read_once_per_batch() {
    let mut volume = CountingMft::shared_chain(5);
    let found = metadata_parents(&mut volume, &spread_batch(), &restored());
    assert!(found.is_empty());
    assert_eq!(volume.reads, PARENTS as usize + ANCESTORS.len());
}

/// A walk that meets a known metadata folder stops there and reads nothing above it.
#[test]
fn a_walk_stops_at_a_known_metadata_folder() {
    let mut index = restored();
    index.note_metadata_folders([ANCESTORS[0]]);
    let mut volume = CountingMft::shared_chain(EXTEND);
    let batch = UsnBatch {
        next_usn: 120,
        records: vec![record(100_000, parent(0), 0x2, "f")],
    };
    assert_eq!(
        metadata_parents(&mut volume, &batch, &index),
        vec![parent(0)]
    );
    assert_eq!(volume.reads, 1, "only the folder itself");
}

/// A read error in the shared ancestor confirms none of the folders below it (fails closed) and
/// ends the search, so no later walk reads again.
#[test]
fn a_read_error_in_a_shared_ancestor_confirms_no_dependent() {
    let mut volume = CountingMft::shared_chain(EXTEND);
    volume.fail_at = Some(1003);
    let found = metadata_parents(&mut volume, &spread_batch(), &restored());
    assert!(found.is_empty());
    assert_eq!(volume.reads, 4, "parent, A1, A2, then the failed A3");
}

/// A chain of more than `MAX_CHAIN` unknown records is not metadata; one record shorter is.
#[test]
fn a_walk_longer_than_the_bound_is_not_metadata() {
    assert_eq!(
        MAX_CHAIN,
        ANCESTORS.len() + 2,
        "deep sits exactly at the bound"
    );
    let index = restored();
    let mut volume = CountingMft::shared_chain(EXTEND);
    let batch = |folder| UsnBatch {
        next_usn: 120,
        records: vec![record(100_000, folder, 0x2, "f")],
    };
    let deep: u128 = (1 << 48) | 900;
    volume.rows.insert(900, (deep as u64, parent(0) as u64));
    volume
        .rows
        .insert(901, (((1_u128 << 48) | 901) as u64, deep as u64));
    assert_eq!(
        metadata_parents(&mut volume, &batch(deep), &index),
        vec![deep]
    );
    let deeper: u128 = (1 << 48) | 901;
    assert!(metadata_parents(&mut volume, &batch(deeper), &index).is_empty());
}

/// The bound result does not depend on walk order within one batch: `deep` (exactly at the
/// bound) is metadata and `deeper` (one past it) is not, whichever is walked first.
#[test]
fn the_bound_result_does_not_depend_on_walk_order() {
    let deep: u128 = (1 << 48) | 900;
    let deeper: u128 = (1 << 48) | 901;
    let mut volume = CountingMft::shared_chain(EXTEND);
    volume.rows.insert(900, (deep as u64, parent(0) as u64));
    volume.rows.insert(901, (deeper as u64, deep as u64));
    for order in [[deeper, deep], [deep, deeper]] {
        let batch = UsnBatch {
            next_usn: 120,
            records: order
                .iter()
                .map(|&folder| record(100_000, folder, 0x2, "f"))
                .collect(),
        };
        assert_eq!(
            metadata_parents(&mut volume, &batch, &restored()),
            vec![deep],
            "order {order:?}"
        );
    }
}

/// A middle ancestor whose record slot holds another sequence number ends every walk through
/// it as "not metadata", and no folder below it is confirmed.
#[test]
fn a_sequence_mismatch_at_a_middle_ancestor_is_not_metadata() {
    let mut volume = CountingMft::shared_chain(EXTEND);
    let reused = ((2_u128 << 48) | 1002) as u64;
    volume.rows.insert(1002, (reused, ANCESTORS[2] as u64));
    assert!(metadata_parents(&mut volume, &spread_batch(), &restored()).is_empty());
    assert_eq!(
        volume.reads,
        PARENTS as usize + 2,
        "A1 and the A2 slot once each"
    );
}

/// A record missing part-way up the chain gives "not metadata" for every folder below it.
#[test]
fn a_missing_ancestor_record_is_not_metadata() {
    let mut volume = CountingMft::shared_chain(EXTEND);
    volume.rows.remove(&1003);
    assert!(metadata_parents(&mut volume, &spread_batch(), &restored()).is_empty());
    assert_eq!(
        volume.reads,
        PARENTS as usize + 3,
        "A1, A2 and the missing A3 once each"
    );
}
