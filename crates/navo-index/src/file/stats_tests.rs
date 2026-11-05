#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "the stats test fails on a broken file and prints counts only"
)]

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use super::{Header, load_with_ids, save_with_ids};
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::usn::FrnMap;

const PAGE: u64 = 1024;
const PAGE_BYTES: usize = 6144;
const WIDE: u128 = (1 << 100) | 8;

fn sample() -> (Header, NameTable, FrnMap) {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "sample", Flags::FOLDER).unwrap();
    let docs = table.push(root, "docs", Flags::FOLDER).unwrap();
    let src = table.push(root, "src", Flags::FOLDER).unwrap();
    let names = ["a.txt", "b.txt", "notes.md", "a.txt", "main.rs", "lib.rs"];
    let files: Vec<u32> = names
        .iter()
        .enumerate()
        .map(|(n, name)| {
            let parent = if n % 2 == 0 { docs } else { src };
            table.push(parent, name, Flags::NONE).unwrap()
        })
        .collect();
    let ids = FrnMap::from([
        (5u128, root),
        (100, docs),
        (101, files[0]),
        (WIDE, files[1]),
    ]);
    let header = Header {
        volume_serial: 1,
        journal_id: 2,
        next_usn: 3,
        root: "sample".to_owned(),
        built_at: 0,
    };
    (header, table, ids)
}

fn per_cent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

fn stat(key: &str, value: impl std::fmt::Display) {
    println!("STATS {key} {value}");
}

#[test]
#[ignore = "real snapshot stats"]
fn snapshot_stats() {
    let folder = tempfile::tempdir().unwrap();
    let path = match std::env::var_os("NAVO_INDEX_STATS") {
        Some(path) => Path::new(&path).to_path_buf(),
        None => {
            let (header, table, ids) = sample();
            let path = folder.path().join("sample.index");
            save_with_ids(&path, &header, &table, &ids).unwrap();
            path
        }
    };
    let (_, table, ids) = load_with_ids(&path).unwrap();
    print_names(&table);
    print_references(&ids);
}

fn print_names(table: &NameTable) {
    let mut live_bytes = 0;
    let mut unique = HashSet::new();
    let mut unique_bytes = 0;
    for (_, name) in table.iter() {
        live_bytes += name.len() + 1;
        if unique.insert(name.to_owned()) {
            unique_bytes += name.len() + 1;
        }
    }
    stat("slots", table.slots());
    stat("live", table.len());
    stat("folders", table.folder_count());
    stat("arena_bytes", table.raw_parts().0.len());
    stat("live_name_bytes", live_bytes);
    stat("unique_live_name_bytes", unique_bytes);
    stat(
        "dedup_saving_percent",
        format!("{:.1}", per_cent(live_bytes - unique_bytes, live_bytes)),
    );
    stat(
        "mean_name_bytes",
        format!("{:.2}", live_bytes as f64 / table.len().max(1) as f64),
    );
}

fn print_references(ids: &FrnMap) {
    let mut records: BTreeMap<u64, u32> = BTreeMap::new();
    let mut wide = 0;
    for (reference, _) in ids.iter() {
        match u64::try_from(reference) {
            Ok(narrow) => *records.entry(narrow & 0xFFFF_FFFF_FFFF).or_default() += 1,
            Err(_) => wide += 1,
        }
    }
    let narrow: usize = records.values().map(|&count| count as usize).sum();
    let pages: HashSet<u64> = records
        .keys()
        .filter(|&&record| record < 1 << 32)
        .map(|record| record / PAGE)
        .collect();

    stat("references", ids.len());
    stat("wide_references", wide);
    stat(
        "max_record",
        records.keys().next_back().copied().unwrap_or(0),
    );
    stat("dense_pages", pages.len());
    stat(
        "dense_bytes_per_reference",
        format!(
            "{:.2}",
            (pages.len() * PAGE_BYTES) as f64 / narrow.max(1) as f64
        ),
    );
    stat(
        "records_with_several_references",
        records.values().filter(|&&count| count > 1).count(),
    );
}
