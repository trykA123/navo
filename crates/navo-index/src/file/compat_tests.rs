use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::{Header, load_with_ids, save_with_ids};
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::usn::FrnMap;

const RECORD_300_SEQ_1: u128 = (1 << 48) | 300;
const RECORD_300_SEQ_2: u128 = (2 << 48) | 300;
const RECORD_2_32: u128 = (1 << 32) + 1;
const RECORD_2_40: u128 = 1 << 40;
const WIDE_A: u128 = (1 << 100) | 8;
const WIDE_B: u128 = (1 << 100) | 9;

const RICH_V3_HEX: &str = concat!(
    "464552524954452d494e444558000000030007000000000000000b00000000000000640000000000000000000000",
    "0000000002000000bc000000150000000c000000f2b248db433a433a00557365727300576f726b00417263686976",
    "65005472617368006e6f7465732e6d6400736f6e672e6d7033006c6f672e747874006f6c642e62696e007265706f",
    "72742e747874007265706f72742e747874006e6f7465732e6d6400646174612e62696e00696d6167652e6a706700",
    "726561646d652e747874006d61696e2e7273006c69622e727300636f6e6669672e746f6d6c006261636b75702e7a",
    "69700064726166742e6d6400676f6e652e7478740066696e616c2e6d64000000000003000000090000000e000000",
    "160000001c000000250000002e000000360000003e0000003e0000001c0000005d00000066000000700000007b00",
    "0000830000008a00000096000000b3000000aa000000ffffffff0000000000000000000000000000000003000000",
    "03000000030000000400000001000000020000000200000001000000030000000000000003000000030000000200",
    "00000100000002000000010000000100010001000100110000000000000010000000000000000000040000000000",
    "00000200000000001000c63b000000000000de0c0000000000004b110000000000004c1900000000000000000000",
    "0000000012040000000000001904000000000000200400000000000000000000000000002e040000000000003504",
    "0000000000003c0400000000000043040000000000004a0400000000000051040000000000005804000000000000",
    "5f0400000000000066040000000000006d04000000000000740400000000000000000000000000000e0000000300",
    "00000300000000000000040000000000000006000000000000000000000000000000000000000000000000000000",
    "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "000000000000000000000000000000000000000000000000650000000000000000000000000000000a0000000500",
    "000000000000000000000000000000000000010000000100000000000000000000000f0000006600000000000000",
    "00000000000000000b00000000000000000100000000000000000000100000006400000000000000000000000000",
    "00000900000006000000000000000000000000000000000000002c0100000000010000000000000000000d000000",
    "2c0100000000020000000000000000000e000000670000000000000000000000000000000c000000080000000000",
    "00000000000010000000110000000900000000000000000000001000000012000000",
);

type Slot = Option<(String, u32, u16, u64, Option<[u32; 2]>)>;

#[derive(Debug, PartialEq, Eq)]
struct Semantics {
    slots: u32,
    entries: Vec<Slot>,
    references: BTreeSet<(u128, u32)>,
}

struct Folders {
    users: u32,
    work: u32,
    archive: u32,
}

struct Files {
    report_users: u32,
    report_work: u32,
    notes_work: u32,
    data: u32,
    image: u32,
    readme: u32,
    main: u32,
    lib: u32,
    config: u32,
    backup: u32,
    draft: u32,
    gone: u32,
}

fn header() -> Header {
    Header {
        volume_serial: 7,
        journal_id: 11,
        next_usn: 100,
        root: "C:".to_owned(),
        built_at: 0,
    }
}

fn rich() -> (Header, NameTable, FrnMap) {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let folders = Folders {
        users: table.push(root, "Users", Flags::FOLDER).unwrap(),
        work: table.push(root, "Work", Flags::FOLDER).unwrap(),
        archive: table.push(root, "Archive", Flags::FOLDER).unwrap(),
    };
    let trash = table.push(root, "Trash", Flags::FOLDER).unwrap();
    let files = add_files(&mut table, root, trash, &folders);
    table.rename(files.draft, folders.work, "final.md").unwrap();
    table.remove(files.gone);
    table.remove(trash);
    let ids = references(root, &files);
    (header(), share_offsets(table), ids)
}

fn push_file(table: &mut NameTable, parent: u32, name: &str, flags: Flags) -> u32 {
    let id = table.push(parent, name, flags).unwrap();
    table
        .set_size(id, 1_000 + u64::from(table.slots()) * 7)
        .unwrap();
    id
}

fn add_files(table: &mut NameTable, root: u32, trash: u32, folders: &Folders) -> Files {
    for name in ["notes.md", "song.mp3", "log.txt"] {
        push_file(table, folders.archive, name, Flags::NONE);
    }
    push_file(table, trash, "old.bin", Flags::NONE);
    Files {
        report_users: push_file(table, folders.users, "report.txt", Flags::NONE),
        report_work: push_file(table, folders.work, "report.txt", Flags::NONE),
        notes_work: push_file(table, folders.work, "notes.md", Flags::NONE),
        data: push_file(table, folders.users, "data.bin", Flags::NONE),
        image: push_file(table, folders.archive, "image.jpg", Flags::SYSTEM),
        readme: push_file(table, root, "readme.txt", Flags::NONE),
        main: push_file(table, folders.archive, "main.rs", Flags::NONE),
        lib: push_file(table, folders.archive, "lib.rs", Flags::NONE),
        config: push_file(table, folders.work, "config.toml", Flags::HIDDEN),
        backup: push_file(table, folders.users, "backup.zip", Flags::NONE),
        draft: push_file(table, folders.work, "draft.md", Flags::NONE),
        gone: push_file(table, folders.users, "gone.txt", Flags::NONE),
    }
}

fn references(root: u32, files: &Files) -> FrnMap {
    FrnMap::from([
        (5u128, root),
        (6, root),
        (100, files.report_users),
        (101, files.report_work),
        (102, files.notes_work),
        (103, files.data),
        (RECORD_300_SEQ_1, files.image),
        (RECORD_300_SEQ_2, files.readme),
        (RECORD_2_32, files.main),
        (RECORD_2_40, files.lib),
        (WIDE_A, files.config),
        (WIDE_B, files.backup),
    ])
}

fn share_offsets(table: NameTable) -> NameTable {
    let (names, offsets, parents, flags) = table.raw_parts();
    let (sizes, counts) = table.stored_totals();
    let mut shared = offsets.to_vec();
    let mut first: HashMap<&str, u32> = HashMap::new();
    for id in 0..table.slots() {
        let Some(name) = table.name(id) else {
            continue;
        };
        match first.get(name) {
            Some(&offset) => shared[id as usize] = offset,
            None => {
                first.insert(name, offsets[id as usize]);
            }
        }
    }
    NameTable::from_raw_parts(
        names.to_owned(),
        (shared, parents.to_vec(), flags.to_vec()),
        (sizes.to_vec(), counts.collect()),
    )
    .expect("shared offsets keep every stored name valid")
}

fn assert_shared_offsets(table: &NameTable) {
    let (_, offsets, ..) = table.raw_parts();
    let mut seen = BTreeSet::new();
    let shared = (0..table.slots())
        .filter(|&id| table.flags(id).is_some())
        .filter(|&id| !seen.insert(offsets[id as usize]))
        .count();
    assert!(shared >= 2, "two live ids must share an arena offset");
}

fn semantics(table: &NameTable, ids: &FrnMap) -> Semantics {
    let (_, _, parents, _) = table.raw_parts();
    let entries = (0..table.slots())
        .map(|id| {
            let flags = table.flags(id)?;
            let counts = table
                .totals(id)
                .map(|totals| [totals.files, totals.folders]);
            Some((
                table.name(id)?.to_owned(),
                parents[id as usize],
                flags.bits(),
                table.size(id)?,
                counts,
            ))
        })
        .collect();
    Semantics {
        slots: table.slots(),
        entries,
        references: ids.iter().collect(),
    }
}

fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect()
}

#[test]
fn rich_round_trips() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("rich.index");
    let (header, table, ids) = rich();
    assert_shared_offsets(&table);
    save_with_ids(&path, &header, &table, &ids).unwrap();
    let (loaded_header, loaded_table, loaded_ids) = load_with_ids(&path).unwrap();
    assert_eq!(loaded_header, header);
    assert_eq!(
        semantics(&loaded_table, &loaded_ids),
        semantics(&table, &ids)
    );
    assert_shared_offsets(&loaded_table);
}

#[test]
fn stored_rich_v3_loads() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("stored.index");
    let bytes = unhex(RICH_V3_HEX);
    assert_eq!(u16::from_le_bytes([bytes[16], bytes[17]]), 3);
    std::fs::write(&path, bytes).unwrap();
    let (_, table, ids) = load_with_ids(&path).unwrap();
    let (_, expected_table, expected_ids) = rich();
    assert_eq!(
        semantics(&table, &ids),
        semantics(&expected_table, &expected_ids)
    );
}

#[test]
#[ignore = "cross-version write"]
fn compat_write() {
    let path =
        std::env::var_os("NAVO_COMPAT_OUT").expect("NAVO_COMPAT_OUT must name the output file");
    let (header, table, ids) = rich();
    save_with_ids(Path::new(&path), &header, &table, &ids).unwrap();
}

#[test]
#[ignore = "cross-version read"]
fn compat_read() {
    let path = std::env::var_os("NAVO_COMPAT_IN").expect("NAVO_COMPAT_IN must name the input file");
    let (_, table, ids) = load_with_ids(Path::new(&path)).unwrap();
    let (_, expected_table, expected_ids) = rich();
    assert_eq!(
        semantics(&table, &ids),
        semantics(&expected_table, &expected_ids)
    );
}
