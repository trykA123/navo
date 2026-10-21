use std::{fs, path::Path, time::Instant};

use super::{CHECKSUM_START, FORMAT_VERSION, FileError, HEADER_LEN, Header, load, save};
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::usn::FrnMap;

fn header() -> Header {
    Header {
        volume_serial: 0x0123_4567_89ab_cdef,
        journal_id: 0xfedc_ba98_7654_3210,
        next_usn: 0x1234_5678_9abc,
        root: "fixture/資料".to_owned(),
        built_at: 1_790_000_000,
    }
}

fn table() -> NameTable {
    let mut table = NameTable::new();
    table.push(NO_PARENT, "fixture", Flags::FOLDER).unwrap();
    table
        .push(0, "資料 🦀", Flags::FOLDER | Flags::HIDDEN)
        .unwrap();
    table.push(1, "old.txt", Flags::SYSTEM).unwrap();
    table.push(1, "removed.txt", Flags::NONE).unwrap();
    table.push(0, "link", Flags::REPARSE).unwrap();
    table.rename(2, 0, "new.txt").unwrap();
    table.remove(3);
    table
}

fn assert_table_matches(expected: &NameTable, actual: &NameTable) {
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual.slots(), expected.slots());
    for id in 0..expected.slots() {
        assert_eq!(actual.name(id), expected.name(id), "name at {id}");
        assert_eq!(actual.parent(id), expected.parent(id), "parent at {id}");
        assert_eq!(actual.flags(id), expected.flags(id), "flags at {id}");
    }
}

fn write_fixture(path: &Path) -> Vec<u8> {
    save(path, &header(), &table()).unwrap();
    fs::read(path).unwrap()
}

#[test]
fn round_trip_preserves_header_names_parents_flags_and_removed_slots() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("names.index");
    let expected = table();
    save(&path, &header(), &expected).unwrap();
    let (actual_header, mut actual) = load(&path).unwrap();
    assert_eq!(actual_header, header());
    assert_table_matches(&expected, &actual);
    assert_eq!(
        actual.path_with(2, '\\').as_deref(),
        Some("fixture\\new.txt")
    );
    assert_eq!(actual.push(0, "later", Flags::NONE).unwrap(), 5);
    assert_eq!(actual.remove(1), 1);
    assert_eq!(actual.len(), 4);
}

#[test]
fn empty_table_round_trips() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("empty.index");
    save(&path, &header(), &NameTable::new()).unwrap();
    let (actual_header, actual) = load(&path).unwrap();
    assert_eq!(actual_header, header());
    assert!(actual.is_empty());
    assert_eq!(actual.slots(), 0);
}

#[test]
fn frn_map_round_trips_full_width_references_and_root_aliases() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("ntfs.index");
    let expected = table();
    let ids: FrnMap = [(5, 0), ((5 << 48) | 5, 0), ((1u128 << 100) | 9, 2)].into();
    super::save_with_ids(&path, &header(), &expected, &ids).unwrap();
    let (actual_header, actual, actual_ids) = super::load_with_ids(&path).unwrap();
    assert_eq!(actual_header, header());
    assert_table_matches(&expected, &actual);
    assert_eq!(actual_ids, ids);
}

#[test]
fn walk_snapshots_use_the_new_format_with_an_empty_map() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("walk.index");
    write_fixture(&path);
    let (_, actual, ids) = super::load_with_ids(&path).unwrap();
    assert_table_matches(&table(), &actual);
    assert!(ids.is_empty());
}

fn write_frn_fixture(path: &Path) -> Vec<u8> {
    let ids: FrnMap = [(5, 0), ((1u128 << 100) | 9, 2)].into();
    super::save_with_ids(path, &header(), &table(), &ids).unwrap();
    fs::read(path).unwrap()
}

#[test]
fn every_truncated_frn_snapshot_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("ntfs.index");
    let bytes = write_frn_fixture(&path);
    for length in 0..bytes.len() {
        fs::write(&path, &bytes[..length]).unwrap();
        assert!(
            super::load_with_ids(&path).is_err(),
            "accepted {length} bytes"
        );
    }
}

#[test]
fn loading_refuses_an_out_of_range_entry_id_with_a_valid_checksum() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("ntfs.index");
    let mut bytes = write_frn_fixture(&path);
    let at = bytes.len() - 4;
    bytes[at..].copy_from_slice(&table().slots().to_le_bytes());
    repair_checksum(&mut bytes);
    fs::write(&path, bytes).unwrap();
    assert!(matches!(
        super::load_with_ids(&path),
        Err(FileError::InvalidData(_))
    ));
}

#[test]
fn saving_refuses_an_out_of_range_entry_id() {
    let folder = tempfile::tempdir().unwrap();
    let ids = [(5, table().slots())].into();
    assert!(
        super::save_with_ids(&folder.path().join("ntfs.index"), &header(), &table(), &ids).is_err()
    );
}

#[test]
fn duplicate_file_references_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("ntfs.index");
    let mut bytes = write_frn_fixture(&path);
    let at = bytes.len() - 40;
    let first = bytes[at..at + 16].to_vec();
    bytes[at + 20..at + 36].copy_from_slice(&first);
    repair_checksum(&mut bytes);
    fs::write(&path, bytes).unwrap();
    assert!(matches!(
        super::load_with_ids(&path),
        Err(FileError::InvalidData(_))
    ));
}

#[test]
fn section_counts_must_match_the_file_length() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("ntfs.index");
    let original = write_frn_fixture(&path);
    for at in [50, 54, 58, 62] {
        for count in [0u32, 1, u32::MAX] {
            let mut bytes = original.clone();
            bytes[at..at + 4].copy_from_slice(&count.to_le_bytes());
            repair_checksum(&mut bytes);
            fs::write(&path, bytes).unwrap();
            assert!(
                super::load_with_ids(&path).is_err(),
                "accepted count {count} at {at}"
            );
        }
    }
}

#[test]
fn saving_replaces_the_previous_snapshot() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("names.index");
    write_fixture(&path);
    let mut new_header = header();
    new_header.next_usn += 500;
    save(&path, &new_header, &NameTable::new()).unwrap();
    let (actual_header, actual) = load(&path).unwrap();
    assert_eq!(actual_header, new_header);
    assert!(actual.is_empty());
}

#[test]
fn every_truncated_file_is_refused() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("truncated.index");
    let bytes = write_fixture(&path);
    for length in 0..bytes.len() {
        fs::write(&path, &bytes[..length]).unwrap();
        assert!(load(&path).is_err(), "accepted {length} bytes");
    }
}

#[test]
fn old_and_future_versions_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("version.index");
    let mut bytes = write_fixture(&path);
    for version in [FORMAT_VERSION - 1, FORMAT_VERSION + 1] {
        bytes[16..18].copy_from_slice(&version.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        assert!(matches!(
            load(&path),
            Err(FileError::UnsupportedVersion { found, .. }) if found == version
        ));
    }
}

#[test]
fn checksum_refuses_each_corrupted_byte() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("checksum.index");
    let original = write_fixture(&path);
    for index in 0..original.len() {
        let mut bytes = original.clone();
        bytes[index] ^= 0x80;
        fs::write(&path, &bytes).unwrap();
        assert!(load(&path).is_err(), "accepted changed byte {index}");
    }
}

#[test]
fn trailing_bytes_are_refused() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("length.index");
    let mut bytes = write_fixture(&path);
    bytes.push(0);
    fs::write(&path, bytes).unwrap();
    assert!(matches!(load(&path), Err(FileError::BadLength)));
}

#[test]
fn changed_name_has_a_bad_checksum() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("name.index");
    let mut bytes = write_fixture(&path);
    bytes[HEADER_LEN + header().root.len()] ^= 1;
    fs::write(&path, bytes).unwrap();
    assert!(matches!(load(&path), Err(FileError::BadChecksum)));
}

fn repair_checksum(bytes: &mut [u8]) {
    let mut checksum = crc32fast::Hasher::new();
    checksum.update(&bytes[..CHECKSUM_START]);
    checksum.update(&bytes[HEADER_LEN..]);
    bytes[CHECKSUM_START..HEADER_LEN].copy_from_slice(&checksum.finalize().to_le_bytes());
}

#[test]
fn malformed_data_is_refused_even_with_a_valid_checksum() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("malformed.index");
    let original = write_fixture(&path);
    let names_start = HEADER_LEN + header().root.len();
    let names_len = u32::from_le_bytes(original[54..58].try_into().unwrap()) as usize;
    let offsets_start = names_start + names_len;
    let slots = table().slots() as usize;
    let mutations: &[(usize, &[u8])] = &[
        (HEADER_LEN, &[0xff]),
        (names_start, &[0xff]),
        (offsets_start - 1, b"x"),
        (offsets_start, &u32::MAX.to_le_bytes()),
        (offsets_start, &1u32.to_le_bytes()),
        (offsets_start + 4, &9u32.to_le_bytes()),
        (offsets_start + slots * 8, &u16::MAX.to_le_bytes()),
    ];
    for &(offset, replacement) in mutations {
        let mut bytes = original.clone();
        bytes[offset..offset + replacement.len()].copy_from_slice(replacement);
        repair_checksum(&mut bytes);
        fs::write(&path, &bytes).unwrap();
        assert!(load(&path).is_err(), "accepted invalid data at {offset}");
    }
}

#[test]
fn encoded_metadata_and_arrays_are_little_endian() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("layout.index");
    let bytes = write_fixture(&path);
    let expected = header();
    assert_eq!(&bytes[16..18], &FORMAT_VERSION.to_le_bytes());
    assert_eq!(&bytes[18..26], &expected.volume_serial.to_le_bytes());
    assert_eq!(&bytes[26..34], &expected.journal_id.to_le_bytes());
    assert_eq!(&bytes[34..42], &expected.next_usn.to_le_bytes());
    assert_eq!(&bytes[42..50], &expected.built_at.to_le_bytes());
    assert_eq!(
        &bytes[HEADER_LEN..HEADER_LEN + expected.root.len()],
        expected.root.as_bytes()
    );
    let names_len = u32::from_le_bytes(bytes[54..58].try_into().unwrap()) as usize;
    let offsets_start = HEADER_LEN + expected.root.len() + names_len;
    assert_eq!(
        &bytes[offsets_start + 4..offsets_start + 8],
        &8u32.to_le_bytes()
    );
    let parents_start = offsets_start + table().slots() as usize * 4;
    assert_eq!(
        &bytes[parents_start..parents_start + 4],
        &NO_PARENT.to_le_bytes()
    );
    assert_eq!(
        &bytes[parents_start + 12..parents_start + 16],
        &1u32.to_le_bytes()
    );
    let flags_start = parents_start + table().slots() as usize * 4;
    assert_eq!(
        &bytes[flags_start..flags_start + 2],
        &Flags::FOLDER.bits().to_le_bytes()
    );
}

#[test]
fn missing_file_and_unwritable_destination_report_errors() {
    let folder = tempfile::tempdir().unwrap();
    assert!(load(&folder.path().join("missing.index")).is_err());
    assert!(save(folder.path(), &header(), &table()).is_err());
}

#[test]
#[allow(
    clippy::print_stdout,
    reason = "C1 requires measured save and load times"
)]
fn two_million_entries_save_and_load() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("large.index");
    let mut expected = NameTable::with_capacity(2_000_000, 42_000_000);
    expected.push(NO_PARENT, "fixture", Flags::FOLDER).unwrap();
    for id in 1..2_000_000 {
        expected
            .push(0, &format!("document-{id:07}.txt"), Flags::NONE)
            .unwrap();
    }
    let started = Instant::now();
    save(&path, &header(), &expected).unwrap();
    let save_time = started.elapsed();
    let started = Instant::now();
    let (actual_header, actual) = load(&path).unwrap();
    let load_time = started.elapsed();
    println!("2,000,000 entries: save {save_time:?}, load {load_time:?}");
    assert_eq!(actual_header, header());
    assert_table_matches(&expected, &actual);
}
