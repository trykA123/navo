use super::*;
use std::collections::VecDeque;

use crate::usn::{MAX_CARRIED_READS, UsnRecord};

const ROOT: u128 = (5 << 48) | 5;
const POSITION: JournalPosition = JournalPosition {
    journal_id: 7,
    next_usn: 100,
};

fn record(file_id: u128, parent_id: u128, name: &str) -> UsnRecord {
    UsnRecord {
        size: None,
        file_id,
        parent_id,
        usn: 90,
        reason: 0,
        flags: Flags::FOLDER,
        name: name.into(),
    }
}

fn bytes(cursor: u64, records: &[UsnRecord]) -> Vec<u8> {
    let mut bytes = cursor.to_le_bytes().to_vec();
    for record in records {
        let name: Vec<_> = record.name.encode_utf16().collect();
        let length = (60 + name.len() * 2).next_multiple_of(8);
        let mut wire = vec![0; length];
        wire[..4].copy_from_slice(&(length as u32).to_le_bytes());
        wire[4..6].copy_from_slice(&2u16.to_le_bytes());
        wire[8..16].copy_from_slice(&(record.file_id as u64).to_le_bytes());
        wire[16..24].copy_from_slice(&(record.parent_id as u64).to_le_bytes());
        wire[24..32].copy_from_slice(&record.usn.to_le_bytes());
        wire[40..44].copy_from_slice(&record.reason.to_le_bytes());
        wire[52..56].copy_from_slice(
            &(if record.flags.contains(Flags::FOLDER) {
                0x10u32
            } else {
                0
            })
            .to_le_bytes(),
        );
        wire[56..58].copy_from_slice(&((name.len() * 2) as u16).to_le_bytes());
        wire[58..60].copy_from_slice(&60u16.to_le_bytes());
        let (pairs, _) = wire[60..].as_chunks_mut::<2>();
        for (pair, unit) in pairs.iter_mut().zip(name) {
            pair.copy_from_slice(&unit.to_le_bytes());
        }
        bytes.extend(wire);
    }
    bytes
}

fn journal(id: u64, first: i64, next: i64, lowest: i64) -> Vec<u8> {
    let mut bytes = vec![0; 56];
    bytes[..8].copy_from_slice(&id.to_le_bytes());
    bytes[8..16].copy_from_slice(&first.to_le_bytes());
    bytes[16..24].copy_from_slice(&next.to_le_bytes());
    bytes[24..32].copy_from_slice(&lowest.to_le_bytes());
    bytes
}

struct FakeVolume {
    cancel: Option<crate::pipe::CancelToken>,
    queries: VecDeque<Vec<u8>>,
    buffers: VecDeque<Option<Vec<u8>>>,
    cursors: Vec<u64>,
    reads: Vec<JournalPosition>,
    read: Vec<u8>,
    read_error: Option<i32>,
    sizes: HashMap<u128, u64>,
    size_errors: HashMap<u128, Option<i32>>,
    fallback_error: Option<i32>,
    fallback_reads: Vec<u128>,
    size_reads: Vec<u128>,
    gone: std::collections::HashSet<u128>,
}

impl FakeVolume {
    fn new(buffers: Vec<Option<Vec<u8>>>) -> Self {
        Self {
            cancel: None,
            queries: [journal(7, 10, 100, 1), journal(7, 10, 120, 1)].into(),
            buffers: buffers.into(),
            cursors: Vec::new(),
            reads: Vec::new(),
            read: bytes(120, &[]),
            read_error: None,
            sizes: HashMap::new(),
            size_errors: HashMap::new(),
            fallback_error: None,
            fallback_reads: Vec::new(),
            size_reads: Vec::new(),
            gone: std::collections::HashSet::new(),
        }
    }
}

impl VolumeIo for FakeVolume {
    fn file_size(&mut self, file_id: u128) -> Result<u64, NtfsError> {
        self.size_reads.push(file_id);
        if self.gone.contains(&file_id) {
            return Err(NtfsError::FileGone);
        }
        if let Some(code) = self.size_errors.get(&file_id) {
            return Err(match code {
                Some(code) => NtfsError::Io {
                    operation: "fake size read",
                    source: std::io::Error::from_raw_os_error(*code),
                },
                None => NtfsError::InvalidResponse("malformed file record"),
            });
        }
        Ok(self.sizes.get(&file_id).copied().unwrap_or(file_id as u64))
    }
    fn current_file_size(&mut self, file_id: u128) -> Result<u64, NtfsError> {
        self.fallback_reads.push(file_id);
        if let Some(code) = self.fallback_error {
            return Err(NtfsError::Io {
                operation: "fake fallback",
                source: std::io::Error::from_raw_os_error(code),
            });
        }
        if self.size_errors.get(&file_id).is_some_and(Option::is_none) {
            return Ok(self.sizes.get(&file_id).copied().unwrap_or(file_id as u64));
        }
        self.file_size(file_id)
    }
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        Ok(self.queries.pop_front().unwrap())
    }

    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        self.cursors.push(cursor);
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        Ok(self.buffers.pop_front().unwrap())
    }

    fn read_journal(&mut self, position: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        self.reads.push(position);
        if let Some(code) = self.read_error {
            return Err(NtfsError::Io {
                operation: "read journal",
                source: io::Error::from_raw_os_error(code),
            });
        }
        Ok(self.read.clone())
    }
}

#[test]
fn enumeration_cancellation_discards_partial_results_before_the_next_batch() {
    let cancel = crate::pipe::CancelToken::new();
    let mut volume = FakeVolume::new(vec![Some(bytes(70, &[record(8, ROOT, "first")])), None]);
    volume.cancel = Some(cancel.clone());
    assert!(matches!(
        enumerate_volume(&mut volume, 'C', usize::MAX, &cancel),
        Err(NtfsError::Cancelled)
    ));
    assert_eq!(volume.cursors, [0]);
}

#[test]
fn enumeration_uses_header_cursor_and_keeps_children_until_later_buffers() {
    let mut volume = FakeVolume::new(vec![
        Some(bytes(70, &[record(80, 90, "child")])),
        Some(bytes(
            200,
            &[record(ROOT, ROOT, "."), record(90, ROOT, "parent")],
        )),
        None,
    ]);
    let result = enumerate_volume(
        &mut volume,
        'c',
        usize::MAX,
        &crate::pipe::CancelToken::new(),
    )
    .unwrap();
    assert_eq!(volume.cursors, [0, 70, 200]);
    assert_eq!(result.index.position, POSITION);
    assert!(result.unresolved.is_empty());
    assert_eq!(
        result
            .index
            .table
            .path_with(result.index.ids[&80], '\\')
            .as_deref(),
        Some("C:\\parent\\child")
    );
    assert_eq!(result.index.ids[&ROOT], result.index.ids[&5]);
    assert_eq!(result.index.table.len(), 3);
}

#[test]
fn enumeration_advances_header_only_buffers_without_treating_them_as_eof() {
    let next = (1u64 << 63) | 100;
    let mut volume = FakeVolume::new(vec![Some(bytes(next, &[])), None]);
    enumerate_volume(
        &mut volume,
        'C',
        usize::MAX,
        &crate::pipe::CancelToken::new(),
    )
    .unwrap();
    assert_eq!(volume.cursors, [0, next]);
}

#[test]
fn enumeration_rejects_stalled_or_backward_cursors_and_truncated_records() {
    for buffers in [
        vec![Some(bytes(0, &[]))],
        vec![Some(bytes(9, &[])), Some(bytes(8, &[]))],
        vec![Some(vec![0; 7])],
        vec![Some(vec![0; 9])],
    ] {
        assert!(
            enumerate_volume(
                &mut FakeVolume::new(buffers),
                'C',
                usize::MAX,
                &crate::pipe::CancelToken::new()
            )
            .is_err()
        );
    }
}

#[test]
fn enumeration_reports_orphans_at_the_end_and_respects_the_record_limit() {
    let mut volume = FakeVolume::new(vec![Some(bytes(
        100,
        &[record(8, 9, "orphan"), record(10, ROOT, "known")],
    ))]);
    let result = enumerate_volume(&mut volume, 'C', 1, &crate::pipe::CancelToken::new()).unwrap();
    assert_eq!(result.unresolved.len(), 1);
    assert_eq!(result.unresolved[0].missing_reference, 9);
    assert_eq!(result.index.table.len(), 1);
}

#[test]
fn enumeration_requires_a_rebuild_if_the_journal_changes_or_wraps_during_the_scan() {
    for state in [journal(8, 10, 120, 1), journal(7, 101, 120, 1)] {
        let mut volume = FakeVolume::new(vec![None]);
        volume.queries[1] = state;
        assert!(matches!(
            enumerate_volume(
                &mut volume,
                'C',
                usize::MAX,
                &crate::pipe::CancelToken::new()
            ),
            Err(NtfsError::Reenumerate)
        ));
    }
}

#[test]
fn unknown_rename_becomes_add_and_repeated_creates_update_one_entry() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut old = record(8, ROOT, "old");
    old.reason = 0x1000;
    let mut new = record(8, ROOT, "new");
    new.reason = 0x2000;
    let mut repeated = new.clone();
    repeated.reason = 0x100 | 0x8000_0000;
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![old, new, repeated],
        })
        .unwrap();
    assert!(unresolved.is_empty());
    assert_eq!(index.table.len(), 2);
    assert_eq!(index.table.name(index.ids[&8]), Some("new"));
    assert_eq!(index.position.next_usn, 110);
}

fn create(file_id: u128, parent_id: u128, name: &str) -> UsnRecord {
    let mut created = record(file_id, parent_id, name);
    created.reason = 0x100;
    created
}

fn folder_with_child() -> NtfsIndex {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![create(8, ROOT, "old"), create(9, 8, "child")],
        })
        .unwrap();
    index
}

#[test]
fn a_folder_renamed_across_two_reads_keeps_its_children() {
    let mut index = folder_with_child();
    let mut old = record(8, ROOT, "old");
    old.reason = 0x1000;
    let mut new = record(8, ROOT, "new");
    new.reason = 0x2000;
    index
        .apply_batch(UsnBatch {
            next_usn: 120,
            records: vec![old],
        })
        .unwrap();
    assert_eq!(index.table.name(index.ids[&8]), Some("old"));
    index
        .apply_batch(UsnBatch {
            next_usn: 130,
            records: vec![new],
        })
        .unwrap();
    assert_eq!(index.table.name(index.ids[&8]), Some("new"));
    assert_eq!(index.table.parent(index.ids[&9]), Some(index.ids[&8]));
    assert_eq!(index.table.len(), 3);
}

#[test]
fn restarting_the_journal_forgets_waiting_old_names() {
    let mut index = folder_with_child();
    let mut old = record(8, ROOT, "old");
    old.reason = 0x1000;
    index
        .apply_batch(UsnBatch {
            next_usn: 120,
            records: vec![old],
        })
        .unwrap();
    index.restart_at(POSITION);
    let mut next = POSITION.next_usn;
    for _ in 0..=MAX_CARRIED_READS + 1 {
        next += 10;
        index
            .apply_batch(UsnBatch {
                next_usn: next,
                records: vec![create(20, ROOT, "other")],
            })
            .unwrap();
    }
    assert_eq!(index.table.len(), 4);
}

#[test]
fn deferred_changes_for_one_file_keep_their_order() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut child = record(8, 9, "child");
    child.reason = 0x100;
    let mut removed = child.clone();
    removed.reason = 0x200;
    let mut parent = record(9, ROOT, "parent");
    parent.reason = 0x100;
    assert!(
        index
            .apply_batch(UsnBatch {
                next_usn: 110,
                records: vec![child, removed, parent]
            })
            .unwrap()
            .is_empty()
    );
    assert!(!index.ids.contains_key(&8));
    assert_eq!(index.table.len(), 2);
}

#[test]
fn journal_reads_use_saved_position_and_refuse_stale_or_future_positions() {
    let mut volume = FakeVolume::new(vec![]);
    let batch = read_volume_journal(&mut volume, POSITION).unwrap();
    assert_eq!(batch.next_usn, 120);
    assert_eq!(volume.reads, [POSITION]);
    for state in [
        journal(8, 1, 120, 1),
        journal(7, 101, 120, 1),
        journal(7, 1, 120, 101),
        journal(7, 1, 99, 1),
    ] {
        let mut volume = FakeVolume::new(vec![]);
        volume.queries[0] = state;
        assert!(matches!(
            read_volume_journal(&mut volume, POSITION),
            Err(NtfsError::Reenumerate)
        ));
        assert!(volume.reads.is_empty());
    }
}

#[test]
fn journal_batches_advance_only_with_valid_cursors() {
    for (next, records, valid) in [
        (100, vec![], true),
        (99, vec![], false),
        (100, vec![record(8, ROOT, "entry")], false),
        (101, vec![record(8, ROOT, "entry")], true),
    ] {
        let mut volume = FakeVolume::new(vec![]);
        volume.read = bytes(next, &records);
        assert_eq!(read_volume_journal(&mut volume, POSITION).is_ok(), valid);
    }
}

#[test]
fn requests_use_v0_wire_layouts_and_nonblocking_journal_reads() {
    let request = enum_request(123);
    assert_eq!(request.len(), 24);
    assert_eq!(&request[..8], &123u64.to_le_bytes());
    assert_eq!(&request[8..16], &0i64.to_le_bytes());
    assert_eq!(&request[16..24], &i64::MAX.to_le_bytes());
    let request = journal_request(POSITION);
    assert_eq!(request.len(), 40);
    assert_eq!(&request[..8], &100i64.to_le_bytes());
    assert_eq!(&request[8..12], &u32::MAX.to_le_bytes());
    assert_eq!(&request[12..32], &[0; 20]);
    assert_eq!(&request[32..40], &7u64.to_le_bytes());
}

#[test]
fn query_metadata_and_drive_letters_are_validated() {
    assert!(JournalState::parse(&[0; 55]).is_err());
    assert!(JournalState::parse(&journal(7, 100, 99, 1)).is_err());
    assert!(JournalState::parse(&journal(7, -1, 100, 1)).is_err());
    assert!(NtfsIndex::new('/', POSITION).is_err());
    assert!(volume_path('é').is_err());
    assert_eq!(
        String::from_utf16(&volume_path('c').unwrap()).unwrap(),
        "\\\\.\\C:\0"
    );
}

#[test]
fn journal_reports_missing_parents_without_inserting_orphan_entries() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut missing = record(8, 9, "orphan");
    missing.reason = 0x2000;
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![missing],
        })
        .unwrap();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].missing_reference, 9);
    assert_eq!(index.table.len(), 1);
    assert_eq!(index.position.next_usn, 110);
}

#[test]
fn only_the_ntfs_root_alias_loses_its_sequence_bits() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let file_id = (12 << 48) | 8;
    let wide_id = (1u128 << 100) | 5;
    let mut first = record(file_id, ROOT, "first");
    first.reason = 0x100;
    let mut second = record(wide_id, 5, "second");
    second.reason = 0x100;
    assert!(
        index
            .apply_batch(UsnBatch {
                next_usn: 110,
                records: vec![first, second],
            })
            .unwrap()
            .is_empty()
    );
    assert_eq!(index.table.len(), 3);
    assert!(index.ids.contains_key(&file_id));
    assert!(index.ids.contains_key(&wide_id));
    assert!(!index.ids.contains_key(&8));
}

#[test]
fn journal_keeps_the_checkpoint_when_a_table_change_fails() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut parent = record(8, ROOT, "parent");
    parent.reason = 0x100;
    let mut child = record(9, 8, "child");
    child.reason = 0x100;
    index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![parent.clone(), child],
        })
        .unwrap();
    let mut old = parent.clone();
    old.reason = 0x1000;
    parent.reason = 0x2000;
    parent.parent_id = 9;
    assert!(matches!(
        index.apply_batch(UsnBatch {
            next_usn: 120,
            records: vec![old, parent]
        }),
        Err(NtfsError::Table(TableError::Cycle(_)))
    ));
    assert_eq!(index.position.next_usn, 110);
}

#[test]
fn journal_wrap_between_query_and_read_requires_reenumeration() {
    let mut volume = FakeVolume::new(vec![]);
    volume.read_error = Some(1181);
    assert!(matches!(
        read_volume_journal(&mut volume, POSITION),
        Err(NtfsError::Reenumerate)
    ));
    volume.read_error = Some(5);
    assert!(matches!(
        read_volume_journal(&mut volume, POSITION),
        Err(NtfsError::Io { .. })
    ));
}

#[test]
fn root_metadata_keeps_the_volume_name_and_directory_flag() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut root = record(ROOT, ROOT, ".");
    root.flags = Flags::SYSTEM;
    root.reason = 0x100;
    assert!(
        index
            .apply_batch(UsnBatch {
                next_usn: 110,
                records: vec![root]
            })
            .unwrap()
            .is_empty()
    );
    let id = index.ids[&ROOT];
    assert_eq!(index.table.name(id), Some("C:"));
    assert!(
        index
            .table
            .flags(id)
            .unwrap()
            .contains(Flags::FOLDER | Flags::SYSTEM)
    );
}

#[test]
fn unknown_attributes_are_reported_without_blocking_a_later_create() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut attributes = record(8, ROOT, "entry");
    attributes.reason = 0x8000;
    let mut create = attributes.clone();
    create.reason = 0x100;
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![attributes, create],
        })
        .unwrap();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].missing_reference, 8);
    assert_eq!(index.table.name(index.ids[&8]), Some("entry"));
}

#[cfg(windows)]
#[test]
#[ignore = "requires an elevated Windows process and an active NTFS system-volume USN journal"]
fn windows_system_volume_first_records() {
    use std::mem::{offset_of, size_of};
    use windows_sys::Win32::System::Ioctl::{
        MFT_ENUM_DATA_V0, READ_USN_JOURNAL_DATA_V0, USN_JOURNAL_DATA_V0,
    };

    assert_eq!(size_of::<MFT_ENUM_DATA_V0>(), enum_request(0).len());
    assert_eq!(offset_of!(MFT_ENUM_DATA_V0, HighUsn), 16);
    assert_eq!(
        size_of::<READ_USN_JOURNAL_DATA_V0>(),
        journal_request(POSITION).len()
    );
    assert_eq!(offset_of!(READ_USN_JOURNAL_DATA_V0, UsnJournalID), 32);
    assert_eq!(size_of::<USN_JOURNAL_DATA_V0>(), 56);
    assert_eq!(offset_of!(USN_JOURNAL_DATA_V0, LowestValidUsn), 24);

    let system_drive = std::env::var("SystemDrive").unwrap();
    let drive = system_drive.chars().next().unwrap();
    let mut volume = Volume::open(drive).unwrap();
    let first = VolumeIo::enumerate(&mut volume, 0).unwrap().unwrap();
    assert!(
        parse_batch(&first)
            .unwrap()
            .records
            .iter()
            .any(|entry| is_root(entry.file_id) && entry.flags.contains(Flags::FOLDER))
    );

    let Enumeration { index, .. } =
        enumerate_volume(&mut volume, drive, 4096, &crate::pipe::CancelToken::new()).unwrap();
    let root = index.ids[&5];
    assert!(index.table.flags(root).unwrap().contains(Flags::FOLDER));
    let (folder, _) = index
        .table
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case("$Extend"))
        .unwrap();
    assert!(index.table.flags(folder).unwrap().contains(Flags::FOLDER));
    assert_eq!(index.table.parent(folder), Some(root));
    let batch = volume.read_journal(index.position).unwrap();
    assert!(batch.next_usn >= index.position.next_usn);
}

fn two_million_entry_index() -> (NtfsIndex, Vec<(u128, Vec<u128>)>) {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut next_id = 100u128;
    let mut leaves = Vec::new();
    let mut level = vec![(ROOT_REFERENCE, 0u32)];
    for depth in 0..5 {
        let mut children = Vec::new();
        for &(_, parent) in &level {
            for slot in 0..10 {
                let id = index
                    .table
                    .push(parent, &format!("folder-{depth}-{slot}"), Flags::FOLDER)
                    .unwrap();
                index.ids.insert(next_id, id);
                children.push((next_id, id));
                next_id += 1;
            }
        }
        level = children;
    }
    for &(folder_ref, folder) in &level {
        let mut files = Vec::new();
        for slot in 0..19 {
            let id = index
                .table
                .push(folder, &format!("report-{slot}.docx"), Flags::NONE)
                .unwrap();
            index.ids.insert(next_id, id);
            files.push(next_id);
            next_id += 1;
        }
        leaves.push((folder_ref, files));
    }
    assert!(index.table.len() > 2_000_000);
    (index, leaves)
}

#[test]
fn a_journal_read_that_deletes_thirty_folders_applies_within_50ms_on_a_two_million_entry_index() {
    let (mut index, leaves) = two_million_entry_index();
    let mut records = Vec::new();
    for (folder, files) in leaves.iter().take(30) {
        for &file in files {
            let mut deleted = record(file, *folder, "report.docx");
            deleted.reason = FILE_DELETE_REASON;
            records.push(deleted);
        }
        let mut deleted = record(*folder, ROOT, "folder");
        deleted.reason = FILE_DELETE_REASON;
        records.push(deleted);
    }
    let started = std::time::Instant::now();
    index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records,
        })
        .unwrap();
    let held = started.elapsed();
    assert!(
        held < std::time::Duration::from_millis(50),
        "the write lock would be held for {held:?}"
    );
}

const FILE_DELETE_REASON: u32 = 0x200;

#[test]
fn removing_a_folder_with_twenty_thousand_entries_from_a_two_million_entry_index_takes_under_50ms()
{
    let (mut index, _) = two_million_entry_index();
    let before = index.table.len();
    let folder = index.ids[&110];
    let started = std::time::Instant::now();
    let removed = index.table.remove(folder);
    let held = started.elapsed();
    assert_eq!(removed, 1 + 10 + 100 + 1_000 + 19_000);
    assert_eq!(before - index.table.len(), removed);
    assert!(
        held < std::time::Duration::from_millis(50),
        "the write lock would be held for {held:?}"
    );
}

#[test]
fn a_journal_read_that_deletes_a_folder_drops_the_file_references_of_its_subtree() {
    let (mut index, leaves) = two_million_entry_index();
    let (folder, files) = &leaves[0];
    let mut deleted = record(*folder, ROOT, "folder");
    deleted.reason = FILE_DELETE_REASON;
    index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![deleted],
        })
        .unwrap();
    assert!(files.iter().all(|file| !index.ids.contains_key(file)));
    assert!(index.ids.values().all(|&id| index.table.name(id).is_some()));
}

fn sized_index() -> (FakeVolume, NtfsIndex, UsnRecord) {
    let mut file = record(20, 8, "file");
    file.flags = Flags::NONE;
    let mut volume = FakeVolume::new(vec![
        Some(bytes(
            100,
            &[file.clone(), record(8, ROOT, "old"), record(9, ROOT, "new")],
        )),
        None,
    ]);
    volume.sizes.insert(20, 1000);
    let index = enumerate_volume(
        &mut volume,
        'C',
        usize::MAX,
        &crate::pipe::CancelToken::new(),
    )
    .unwrap()
    .index;
    (volume, index, file)
}

#[test]
fn mft_build_and_journal_events_keep_totals_current() {
    let (mut volume, mut index, mut file) = sized_index();
    volume.queries = vec![journal(7, 10, 200, 1); 5].into();
    for (reason, size) in [(1, 20), (2, 2000), (4, 10)] {
        let mut resized = file.clone();
        resized.reason = reason;
        volume.sizes.insert(20, size);
        let next = index.position.next_usn + 10;
        volume.read = bytes(next as u64, &[resized]);
        let batch = read_volume_journal(&mut volume, index.position).unwrap();
        index.apply_batch(batch).unwrap();
        assert_eq!(index.table.totals(0).unwrap().bytes, size);
    }
    let mut moved = record(8, 9, "moved");
    moved.reason = 0x2000;
    let mut old = record(8, ROOT, "old");
    old.reason = 0x1000;
    index
        .apply_batch(UsnBatch {
            next_usn: 150,
            records: vec![old, moved],
        })
        .unwrap();
    assert_eq!(
        index.table.totals(index.ids[&9]).unwrap(),
        crate::table::Totals {
            bytes: 10,
            files: 1,
            folders: 1
        }
    );
    let mut created = file.clone();
    created.file_id = 21;
    created.reason = 0x100;
    volume.sizes.insert(21, 50);
    volume.read = bytes(160, &[created]);
    let batch = read_volume_journal(&mut volume, index.position).unwrap();
    index.apply_batch(batch).unwrap();
    assert_eq!(index.table.totals(0).unwrap().bytes, 60);
    file.reason = 0x200 | 7;
    volume.read = bytes(170, &[file]);
    let batch = read_volume_journal(&mut volume, index.position).unwrap();
    index.apply_batch(batch).unwrap();
    assert_eq!(index.table.totals(0).unwrap().bytes, 50);
    assert_eq!(volume.size_reads, [20, 20, 20, 20, 21]);
}

#[test]
fn mft_build_includes_file_sizes_in_folder_totals() {
    let (_, index, _) = sized_index();
    assert_eq!(
        index.table.totals(0).unwrap(),
        crate::table::Totals {
            bytes: 1000,
            files: 1,
            folders: 2
        }
    );
}

#[test]
fn files_deleted_during_size_reads_do_not_abort_build_or_journal_follow() {
    let mut file = record(20, ROOT, "gone");
    file.flags = Flags::NONE;
    let mut volume = FakeVolume::new(vec![Some(bytes(100, &[file.clone()])), None]);
    volume.gone.insert(20);
    let built = enumerate_volume(
        &mut volume,
        'C',
        usize::MAX,
        &crate::pipe::CancelToken::new(),
    )
    .unwrap();
    assert!(built.unresolved.is_empty());
    assert_eq!(built.index.table.len(), 1);
    file.reason = 0x100;
    volume.queries.push_back(journal(7, 10, 110, 1));
    volume.read = bytes(110, &[file]);
    let batch = read_volume_journal(&mut volume, POSITION).unwrap();
    assert!(matches!(
        batch
            .into_changes(&mut PendingRenames::default())
            .as_slice(),
        [TableChange::Remove { file_id: 20 }]
    ));
}

#[test]
fn a_journal_batch_reads_each_surviving_size_once_and_skips_deleted_files() {
    let mut file = record(20, ROOT, "file");
    file.flags = Flags::NONE;
    file.reason = 1;
    let mut deleted = file.clone();
    deleted.file_id = 21;
    let mut deletion = deleted.clone();
    deletion.reason = 0x200;
    let mut volume = FakeVolume::new(vec![]);
    volume.gone.insert(21);
    volume.read = bytes(110, &[file.clone(), file, deleted, deletion]);
    let batch = read_volume_journal(&mut volume, POSITION).unwrap();
    assert_eq!(volume.size_reads, [20]);
    assert_eq!(batch.records[0].size, Some(20));
    assert_eq!(batch.records[1].size, Some(20));
}

#[test]
fn malformed_record_and_denied_fallback_do_not_stop_enumeration() {
    let mut file = record(20, ROOT, "unknown");
    file.flags = Flags::NONE;
    let mut later = file.clone();
    later.file_id = 21;
    let mut volume = FakeVolume::new(vec![Some(bytes(100, &[file, later])), None]);
    volume.size_errors.insert(20, None);
    volume.fallback_error = Some(5);
    let built = enumerate_volume(
        &mut volume,
        'C',
        usize::MAX,
        &crate::pipe::CancelToken::new(),
    )
    .unwrap();
    assert_eq!(built.index.table.len(), 3);
    assert_eq!(built.index.table.size(built.index.ids[&20]), Some(0));
    assert_eq!(built.index.table.size(built.index.ids[&21]), Some(21));
    assert_eq!(volume.fallback_reads, [20]);
    assert_eq!(built.sizes_unknown, 1);
}

#[test]
fn per_file_journal_errors_keep_the_reason_and_finish_the_batch() {
    for code in [5, 303, 1234] {
        let mut file = record(20, ROOT, "unknown");
        file.flags = Flags::NONE;
        file.reason = 0x100;
        let mut volume = FakeVolume::new(vec![]);
        volume.size_errors.insert(20, Some(code));
        volume.read = bytes(110, &[file.clone(), file]);
        let batch = read_volume_journal(&mut volume, POSITION).unwrap();
        assert!(
            batch
                .records
                .iter()
                .all(|record| record.size.is_none() && record.reason == 0x100)
        );
        assert_eq!(volume.size_reads, [20]);
    }
}

#[test]
fn per_file_io_errors_do_not_stop_enumeration() {
    for code in [5, 303, 1234] {
        let mut file = record(20, ROOT, "unknown");
        file.flags = Flags::NONE;
        let mut volume = FakeVolume::new(vec![Some(bytes(100, &[file])), None]);
        volume.size_errors.insert(20, Some(code));
        let built = enumerate_volume(
            &mut volume,
            'C',
            usize::MAX,
            &crate::pipe::CancelToken::new(),
        )
        .unwrap();
        assert_eq!(built.index.table.size(built.index.ids[&20]), Some(0));
        assert!(volume.fallback_reads.is_empty());
        assert_eq!(built.sizes_unknown, 1);
    }
}

#[test]
fn missing_invalid_or_inaccessible_file_references_become_deletions() {
    for code in [2, 3, 87, 1920] {
        let mut file = record(20, ROOT, "gone");
        file.flags = Flags::NONE;
        file.reason = 0x100;
        let mut volume = FakeVolume::new(vec![]);
        volume.size_errors.insert(20, Some(code));
        volume.read = bytes(110, &[file]);
        let batch = read_volume_journal(&mut volume, POSITION).unwrap();
        assert!(matches!(
            batch
                .into_changes(&mut PendingRenames::default())
                .as_slice(),
            [TableChange::Remove { file_id: 20 }]
        ));
    }
}

#[test]
fn malformed_record_uses_the_current_file_size_when_available() {
    let mut file = record(20, ROOT, "fallback");
    file.flags = Flags::NONE;
    let mut volume = FakeVolume::new(vec![Some(bytes(100, &[file])), None]);
    volume.size_errors.insert(20, None);
    volume.sizes.insert(20, 123);
    let built = enumerate_volume(
        &mut volume,
        'C',
        usize::MAX,
        &crate::pipe::CancelToken::new(),
    )
    .unwrap();
    assert_eq!(built.index.table.size(built.index.ids[&20]), Some(123));
    assert_eq!(volume.fallback_reads, [20]);
    assert_eq!(built.sizes_unknown, 0);
}
