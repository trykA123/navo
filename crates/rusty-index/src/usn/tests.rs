use super::*;
use crate::table::{NameTable, TableChange};
use rusty_core::ProviderPath;

const CREATE: u32 = 0x100;
const DELETE: u32 = 0x200;
const OLD: u32 = 0x1000;
const NEW: u32 = 0x2000;
const BASIC: u32 = 0x8000;
const CLOSE: u32 = 0x8000_0000;

struct Record {
    version: u16,
    file_id: u128,
    parent_id: u128,
    reason: u32,
    attributes: u32,
    name: Vec<u16>,
}

impl Record {
    fn new(version: u16, reason: u32, name: &str) -> Self {
        Self {
            version,
            file_id: 42,
            parent_id: 7,
            reason,
            attributes: 0,
            name: name.encode_utf16().collect(),
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let width = if self.version == 2 { 8 } else { 16 };
        let header = 44 + width * 2;
        let length = (header + self.name.len() * 2).next_multiple_of(8);
        let mut bytes = vec![0; length];
        bytes[..4].copy_from_slice(&(length as u32).to_le_bytes());
        bytes[4..6].copy_from_slice(&self.version.to_le_bytes());
        bytes[8..8 + width].copy_from_slice(&self.file_id.to_le_bytes()[..width]);
        bytes[8 + width..8 + width * 2].copy_from_slice(&self.parent_id.to_le_bytes()[..width]);
        let usn = 8 + width * 2;
        bytes[usn..usn + 8].copy_from_slice(&123i64.to_le_bytes());
        let reason = 24 + width * 2;
        bytes[reason..reason + 4].copy_from_slice(&self.reason.to_le_bytes());
        bytes[reason + 12..reason + 16].copy_from_slice(&self.attributes.to_le_bytes());
        bytes[header - 4..header - 2]
            .copy_from_slice(&((self.name.len() * 2) as u16).to_le_bytes());
        bytes[header - 2..header].copy_from_slice(&(header as u16).to_le_bytes());
        let (pairs, _) = bytes[header..].as_chunks_mut::<2>();
        for (pair, unit) in pairs.iter_mut().zip(&self.name) {
            pair.copy_from_slice(&unit.to_le_bytes());
        }
        bytes
    }
}

fn batch(records: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = 456i64.to_le_bytes().to_vec();
    for record in records {
        bytes.extend(record);
    }
    bytes
}

fn changes(records: &[Record]) -> Vec<TableChange> {
    let records: Vec<_> = records.iter().map(Record::bytes).collect();
    parse_batch(&batch(&records)).unwrap().into_changes()
}

#[test]
fn parses_v2_with_full_64_bit_references_and_directory_attributes() {
    let mut record = Record::new(2, CREATE, "birch");
    record.file_id = u128::from(u64::MAX - 1);
    record.parent_id = u128::from(u64::MAX - 2);
    record.attributes = 0x10 | 0x2 | 0x4 | 0x400;
    let parsed = parse_batch(&batch(&[record.bytes()])).unwrap();
    assert_eq!(parsed.next_usn, 456);
    assert_eq!(parsed.records.len(), 1);
    let record = &parsed.records[0];
    assert_eq!(record.file_id, u128::from(u64::MAX - 1));
    assert_eq!(record.parent_id, u128::from(u64::MAX - 2));
    assert_eq!(record.usn, 123);
    assert_eq!(record.name, "birch");
    assert_eq!(
        record.flags,
        Flags::FOLDER | Flags::HIDDEN | Flags::SYSTEM | Flags::REPARSE
    );
}

#[test]
fn parses_v3_without_truncating_128_bit_references() {
    let mut record = Record::new(3, CREATE, "maple.txt");
    record.file_id = (1u128 << 112) | 42;
    record.parent_id = (1u128 << 104) | 7;
    let parsed = parse_batch(&batch(&[record.bytes()])).unwrap();
    assert_eq!(parsed.records[0].file_id, record.file_id);
    assert_eq!(parsed.records[0].parent_id, record.parent_id);
    assert_eq!(parsed.records[0].name, "maple.txt");
}

fn range_record() -> Vec<u8> {
    let mut bytes = vec![0; 80];
    bytes[..4].copy_from_slice(&80u32.to_le_bytes());
    bytes[4..6].copy_from_slice(&4u16.to_le_bytes());
    bytes[60..62].copy_from_slice(&1u16.to_le_bytes());
    bytes[62..64].copy_from_slice(&16u16.to_le_bytes());
    bytes
}

#[test]
fn skips_v4_ranges_and_keeps_parsing_following_named_records() {
    let parsed = parse_batch(&batch(&[
        range_record(),
        Record::new(3, CREATE, "elm").bytes(),
    ]))
    .unwrap();
    assert_eq!(parsed.records.len(), 1);
    assert_eq!(parsed.records[0].name, "elm");
}

#[test]
fn rejects_ranges_whose_extents_exceed_the_record() {
    let mut bytes = range_record();
    bytes[60..62].copy_from_slice(&2u16.to_le_bytes());
    assert!(parse_batch(&batch(&[bytes])).is_err());
}

#[test]
fn create_adds_and_delete_removes_even_with_accumulated_create_and_close() {
    assert!(matches!(
        changes(&[Record::new(2, CREATE | CLOSE, "ash")]).as_slice(),
        [TableChange::Add { file_id: 42, parent_id: 7, name, flags }]
            if name == "ash" && *flags == Flags::NONE
    ));
    assert_eq!(
        changes(&[Record::new(2, CREATE | DELETE | CLOSE, "ash")]),
        vec![TableChange::Remove { file_id: 42 }]
    );
}

#[test]
fn close_and_data_only_records_only_refresh_tracked_attributes() {
    assert!(changes(&[Record::new(2, 0, "ash")]).is_empty());
    for reason in [CLOSE, 1, 2, 4, 0x10, 0x20, 0x40, CLOSE | 7] {
        assert_eq!(
            changes(&[Record::new(2, reason, "ash")]),
            vec![TableChange::Attributes {
                file_id: 42,
                flags: Flags::NONE
            }]
        );
    }
}

#[test]
fn basic_and_reparse_changes_update_stored_attributes() {
    for reason in [BASIC, BASIC | CLOSE, 0x10_0000] {
        let mut record = Record::new(2, reason, "ash");
        record.attributes = 0x2 | 0x4;
        assert_eq!(
            changes(&[record]),
            vec![TableChange::Attributes {
                file_id: 42,
                flags: Flags::HIDDEN | Flags::SYSTEM,
            }]
        );
    }
}

#[test]
fn pairs_renames_by_file_reference_across_interleaved_records() {
    let old = Record::new(3, OLD, "old");
    let mut other_old = Record::new(3, OLD, "other-old");
    other_old.file_id = 43;
    let mut new = Record::new(3, NEW | CLOSE, "new");
    new.parent_id = 8;
    new.attributes = 0x10;
    let mut other_new = Record::new(3, NEW, "other-new");
    other_new.file_id = 43;
    let parsed = changes(&[old, other_old, new, other_new]);
    assert_eq!(
        parsed,
        vec![
            TableChange::Rename {
                file_id: 42,
                parent_id: 8,
                name: "new".into(),
                flags: Flags::FOLDER,
            },
            TableChange::Rename {
                file_id: 43,
                parent_id: 7,
                name: "other-new".into(),
                flags: Flags::NONE,
            },
        ]
    );
}

#[test]
fn unpaired_old_removes_and_unpaired_new_adds() {
    assert_eq!(
        changes(&[Record::new(2, OLD, "old")]),
        vec![TableChange::Remove { file_id: 42 }]
    );
    assert!(matches!(
        changes(&[Record::new(2, NEW, "new")]).as_slice(),
        [TableChange::Add { name, .. }] if name == "new"
    ));
}

#[test]
fn does_not_leave_a_pending_removal_after_delete_and_recreate() {
    let parsed = changes(&[
        Record::new(2, OLD, "old"),
        Record::new(2, DELETE, "old"),
        Record::new(2, CREATE, "new"),
    ]);
    assert!(matches!(
        parsed.as_slice(),
        [TableChange::Remove { .. }, TableChange::Add { .. }]
    ));
}

#[test]
fn rejects_truncated_prefix_and_record_headers() {
    for length in 0..8 {
        assert!(parse_batch(&vec![0; length]).is_err());
    }
    assert!(parse_batch(&batch(&[vec![1; 7]])).is_err());
    for version in [2u16, 3, 4] {
        let mut bytes = vec![0; 8];
        bytes[..4].copy_from_slice(&8u32.to_le_bytes());
        bytes[4..6].copy_from_slice(&version.to_le_bytes());
        assert!(parse_batch(&batch(&[bytes])).is_err());
    }
}

#[test]
fn rejects_length_past_buffer_and_zero_or_unaligned_lengths() {
    for length in [0u32, 7, 65, u32::MAX] {
        let mut bytes = Record::new(2, CREATE, "ash").bytes();
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        assert!(parse_batch(&batch(&[bytes])).is_err());
    }
    let mut bytes = Record::new(2, CREATE, "ash").bytes();
    bytes.pop();
    assert!(parse_batch(&batch(&[bytes])).is_err());
}

#[test]
fn rejects_odd_name_lengths_and_offsets_and_names_outside_the_record() {
    for version in [2, 3] {
        let fields = if version == 2 { 56 } else { 72 };
        for (at, value) in [
            (fields, 3),
            (fields + 2, 61),
            (fields + 2, 0),
            (fields, 600),
        ] {
            let mut bytes = Record::new(version, CREATE, "ash").bytes();
            bytes[at..at + 2].copy_from_slice(&u16::to_le_bytes(value));
            assert!(parse_batch(&batch(&[bytes])).is_err());
        }
    }
}

#[test]
fn honors_runtime_name_offsets_when_a_minor_version_extends_the_header() {
    for version in [2, 3] {
        let mut bytes = Record::new(version, CREATE, "ash").bytes();
        let header = if version == 2 { 60 } else { 76 };
        bytes.splice(header..header, [0; 8]);
        let length = bytes.len() as u32;
        bytes[..4].copy_from_slice(&length.to_le_bytes());
        bytes[6..8].copy_from_slice(&1u16.to_le_bytes());
        bytes[header - 2..header].copy_from_slice(&((header + 8) as u16).to_le_bytes());
        assert_eq!(
            parse_batch(&batch(&[bytes])).unwrap().records[0].name,
            "ash"
        );
    }
}

#[test]
fn unknown_versions_and_bad_later_records_end_the_batch_with_an_offset() {
    let good = Record::new(2, CREATE, "ash").bytes();
    let mut bad = good.clone();
    bad[4..6].copy_from_slice(&5u16.to_le_bytes());
    let error = parse_batch(&batch(&[good.clone(), bad])).unwrap_err();
    assert_eq!(error.offset, 8 + good.len());
    assert_eq!(error.kind, ParseErrorKind::UnsupportedVersion(5));
}

#[test]
fn journal_identity_and_lowest_valid_usn_decide_whether_to_reenumerate() {
    let saved = JournalPosition {
        journal_id: 17,
        next_usn: 100,
    };
    assert_eq!(saved.resume(17, 100), Resume::Continue);
    assert_eq!(saved.resume(17, 90), Resume::Continue);
    assert_eq!(saved.resume(18, 90), Resume::Reenumerate);
    assert_eq!(saved.resume(17, 101), Resume::Reenumerate);
}

#[test]
fn unpaired_surrogates_round_trip_through_parsing_and_the_table() {
    let mut record = Record::new(3, CREATE, "");
    record.name = vec![0x61, 0xd800, 0x62, 0xdcff, 0xdbff, 0xdc00];
    record.parent_id = record.file_id;
    let expected = record.name.clone();
    let mut table = NameTable::new();
    let mut ids = FrnMap::new();
    for change in changes(&[record]) {
        table.apply(change, &mut ids).unwrap();
    }
    let stored = table.name(ids[&42]).unwrap();
    assert_eq!(ProviderPath::from_text(stored).to_wide(), Some(expected));
}

#[test]
fn fixed_seed_random_and_mutated_buffers_never_panic() {
    let mut seed = 0xa3c5_179b_284d_60efu64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let template = batch(&[
        Record::new(2, OLD, "ash").bytes(),
        range_record(),
        Record::new(3, NEW, "elm").bytes(),
    ]);
    for case in 0..6000 {
        let mut bytes = if case % 2 == 0 {
            (0..random() as usize % 512)
                .map(|_| random() as u8)
                .collect::<Vec<_>>()
        } else {
            template.clone()
        };
        if !bytes.is_empty() {
            let at = random() as usize % bytes.len();
            bytes[at] = random() as u8;
            if case % 3 == 0 {
                bytes.truncate(random() as usize % bytes.len());
            }
        }
        if let Ok(parsed) = parse_batch(&bytes) {
            let _ = parsed.into_changes();
        }
    }
}
