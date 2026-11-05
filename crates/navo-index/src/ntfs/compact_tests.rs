#![allow(
    clippy::unwrap_used,
    reason = "compaction tests fail immediately on broken fixtures"
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::COMPACT_MIN_SLOTS;
use crate::file::{self, Header};
use crate::ntfs::NtfsIndex;
use crate::search::{self, Term};
use crate::table::{Flags, Totals};
use crate::usn::{JournalPosition, UsnBatch, UsnRecord};

const ROOT: u128 = 5;
const CREATE: u32 = 0x100;
const DELETE: u32 = 0x200;
const OLD_NAME: u32 = 0x1000;
const NEW_NAME: u32 = 0x2000;
const FIRST_FOLDER: u128 = 1_000;
const FIRST_FILE: u128 = 1_000_000;
const POSITION: JournalPosition = JournalPosition {
    journal_id: 7,
    next_usn: 100,
};

type Row = (Option<String>, Option<Flags>, Option<u64>, Option<Totals>);

struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

struct Tree {
    index: NtfsIndex,
    folders: Vec<u128>,
    files: Vec<u128>,
    rng: Rng,
}

fn record(file_id: u128, parent_id: u128, reason: u32, name: &str) -> UsnRecord {
    let folder = file_id < FIRST_FILE;
    UsnRecord {
        file_id,
        parent_id,
        usn: 100,
        reason,
        flags: if folder { Flags::FOLDER } else { Flags::NONE },
        name: name.into(),
        size: (!folder && reason & CREATE != 0).then(|| file_size(file_id)),
    }
}

fn file_size(file: u128) -> u64 {
    u64::try_from(file % 9_973).unwrap()
}

fn apply(index: &mut NtfsIndex, records: Vec<UsnRecord>) {
    for chunk in records.chunks(10_000) {
        let next_usn = index.position.next_usn + 1;
        let records = chunk.to_vec();
        let unresolved = index.apply_batch(UsnBatch { next_usn, records }).unwrap();
        assert!(unresolved.is_empty(), "{unresolved:?}");
    }
}

fn header() -> Header {
    Header {
        volume_serial: 42,
        journal_id: POSITION.journal_id,
        next_usn: POSITION.next_usn,
        root: "C:".into(),
        built_at: 0,
    }
}

fn empty() -> NtfsIndex {
    NtfsIndex::new('C', POSITION).unwrap()
}

impl Tree {
    fn new(seed: u64, folders: u128, files: u128) -> Self {
        let mut tree = Self {
            index: empty(),
            folders: vec![ROOT],
            files: Vec::new(),
            rng: Rng(seed),
        };
        let mut records = Vec::new();
        for folder in FIRST_FOLDER..FIRST_FOLDER + folders {
            let parent = tree.folders[tree.rng.below(tree.folders.len())];
            records.push(record(folder, parent, CREATE, &format!("d{folder}")));
            tree.folders.push(folder);
        }
        for file in FIRST_FILE..FIRST_FILE + files {
            let parent = tree.folders[tree.rng.below(tree.folders.len())];
            records.push(record(file, parent, CREATE, &format!("f{file}.txt")));
            tree.files.push(file);
        }
        apply(&mut tree.index, records);
        tree
    }

    fn live(&self, ids: &[u128]) -> Vec<u128> {
        let found = |id: &&u128| self.index.ids.contains_key(id);
        ids.iter().filter(found).copied().collect()
    }

    fn pick(&mut self, ids: &[u128]) -> u128 {
        ids[self.rng.below(ids.len())]
    }

    /// Deletes about `percent` of the files and a few whole folders.
    fn delete(&mut self, percent: usize, folders: usize) {
        let candidates = self.folders[1..].to_vec();
        let mut records: Vec<_> = (0..folders)
            .map(|_| self.pick(&candidates))
            .map(|folder| record(folder, ROOT, DELETE, "gone"))
            .collect();
        for &file in &self.live(&self.files) {
            if self.rng.below(100) < percent {
                records.push(record(file, ROOT, DELETE, "gone"));
            }
        }
        apply(&mut self.index, records);
    }

    /// Moves and renames `count` live files, each as one old/new name pair.
    fn churn_names(&mut self, count: usize, round: u32) -> Vec<UsnRecord> {
        let (files, folders) = (self.live(&self.files), self.live(&self.folders));
        let mut records = Vec::new();
        for _ in 0..count.min(files.len()) {
            let (file, to) = (self.pick(&files), self.pick(&folders));
            records.push(record(file, ROOT, OLD_NAME, "old"));
            records.push(record(file, to, NEW_NAME, &format!("m{round}-{file}.txt")));
        }
        records
    }

    /// A mixed journal batch: creates, deletes, renames and size changes.
    fn mixed(&mut self, round: u32) -> Vec<UsnRecord> {
        let mut records = self.churn_names(50, round);
        let folders = self.live(&self.folders);
        let start = FIRST_FILE * 10 * u128::from(round + 1);
        for file in start..start + 200 {
            let parent = self.pick(&folders);
            records.push(record(file, parent, CREATE, &format!("n{file}.txt")));
        }
        for file in self.live(&self.files).into_iter().take(40) {
            records.push(record(file, ROOT, DELETE, "gone"));
        }
        for file in self.live(&self.files).into_iter().skip(40).take(40) {
            records.push(UsnRecord {
                size: Some(file_size(file) % 31),
                ..record(file, ROOT, 0x2, "")
            });
        }
        records
    }
}

fn listing(index: &NtfsIndex) -> BTreeMap<u128, Row> {
    let table = &index.table;
    let row = |id| {
        (
            table.path(id),
            table.flags(id),
            table.size(id),
            table.totals(id),
        )
    };
    index.ids.iter().map(|(file, id)| (file, row(id))).collect()
}

fn all_paths(index: &NtfsIndex) -> Vec<String> {
    let table = &index.table;
    let mut paths: Vec<_> = table.iter().filter_map(|(id, _)| table.path(id)).collect();
    paths.sort();
    paths
}

fn hits(index: &NtfsIndex, needle: &str) -> Vec<(Option<String>, u32, Vec<u32>)> {
    let table = &index.table;
    search::search(table, &[Term::Name(needle)], 500, &|| false)
        .into_iter()
        .map(|hit| (table.path(hit.id), hit.score, hit.indices))
        .collect()
}

fn folder_totals(index: &NtfsIndex) -> Vec<(String, Option<Totals>)> {
    let mut lookup = index.table.folder_lookup();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut found = Vec::new();
    for (path, flags, ..) in listing(index).into_values() {
        if let Some(path) = path.filter(|_| flags.is_some_and(|f| f.contains(Flags::FOLDER))) {
            let id = lookup.find_folder(&path, deadline).unwrap();
            found.push((path, id.and_then(|id| index.table.totals(id))));
        }
    }
    found
}

fn assert_dense_and_consistent(index: &NtfsIndex) {
    assert_eq!(index.table.slots() as usize, index.table.len());
    assert!(index.table.valid_structure());
    assert_eq!(index.ids.get(&ROOT), Some(&0));
    assert!(index.ids.values().all(|&id| index.table.name(id).is_some()));
}

fn compacted(tree: &mut Tree) -> NtfsIndex {
    let before = tree.index.clone();
    let bytes = tree.index.table.memory_bytes();
    assert!(tree.index.compact_if_sparse(0));
    assert_dense_and_consistent(&tree.index);
    assert!(tree.index.table.memory_bytes() < bytes);
    assert_eq!(listing(&tree.index), listing(&before));
    before
}

#[test]
fn compaction_keeps_every_live_name_parent_size_and_total_on_random_trees() {
    for seed in 1..=16 {
        let mut tree = Tree::new(seed * 7_919, 60, 2_000);
        tree.delete(75, 4);
        let mut records = tree.churn_names(100, 0);
        let files = tree.live(&tree.files);
        let (flipped, moved) = (files[0], files[1]);
        records.push(UsnRecord {
            flags: Flags::FOLDER,
            ..record(flipped, ROOT, 0x4, "")
        });
        records.push(record(moved, ROOT, OLD_NAME, "old"));
        records.push(record(moved, flipped, NEW_NAME, "inside.txt"));
        apply(&mut tree.index, records);
        let flipped_totals = tree.index.table.totals(tree.index.ids[&flipped]);
        assert!(flipped_totals.is_some_and(|totals| totals.bytes >= file_size(moved)));
        let before = compacted(&mut tree);
        assert_eq!(all_paths(&tree.index), all_paths(&before), "seed {seed}");
        assert_eq!(folder_totals(&tree.index), folder_totals(&before));
        assert_eq!(tree.index.ids.len(), before.ids.len());
    }
}

#[test]
fn later_journal_batches_land_on_the_same_entries_as_without_compaction() {
    for seed in 1..=8 {
        let mut tree = Tree::new(seed * 104_729, 40, 1_500);
        tree.delete(80, 2);
        let mut reference = compacted(&mut tree);
        for round in 0..6 {
            let records = tree.mixed(round);
            apply(&mut reference, records.clone());
            apply(&mut tree.index, records);
            assert_eq!(listing(&tree.index), listing(&reference), "seed {seed}");
            assert_eq!(all_paths(&tree.index), all_paths(&reference));
        }
    }
}

#[test]
fn searches_and_folder_lookups_answer_the_same_around_a_compaction() {
    let mut tree = Tree::new(31, 80, 3_000);
    tree.delete(70, 3);
    let before = compacted(&mut tree);
    for needle in ["f1000", "d10", "txt", "f10012", "zzz"] {
        assert_eq!(hits(&tree.index, needle), hits(&before, needle), "{needle}");
    }
    assert!(!hits(&tree.index, "f1000").is_empty());
}

#[test]
fn a_compacted_index_saves_and_restores_with_the_same_listing() {
    let mut tree = Tree::new(53, 50, 2_000);
    tree.delete(75, 2);
    compacted(&mut tree);
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("c.index");
    let (table, ids, position) = tree.index.snapshot().unwrap();
    let header = Header {
        next_usn: position.next_usn,
        ..header()
    };
    file::save_with_ids(&path, &header, table, ids).unwrap();
    let mut restored = NtfsIndex::restore(file::load_with_ids(&path).unwrap(), 42, 7).unwrap();
    assert_eq!(listing(&restored), listing(&tree.index));
    let records = tree.mixed(0);
    apply(&mut restored, records.clone());
    apply(&mut tree.index, records);
    assert_eq!(listing(&restored), listing(&tree.index));
}

#[test]
fn a_file_reference_deleted_then_recreated_across_a_compaction_names_the_new_file() {
    let mut tree = Tree::new(71, 30, 1_000);
    let (gone, kept) = (FIRST_FILE + 3, FIRST_FILE + 4);
    apply(&mut tree.index, vec![record(gone, ROOT, DELETE, "gone")]);
    tree.delete(90, 0);
    apply(
        &mut tree.index,
        vec![record(kept, FIRST_FOLDER, CREATE, "kept.txt")],
    );
    let reference = compacted(&mut tree);
    assert_eq!(tree.index.ids.get(&gone), None);
    let again = record(gone, FIRST_FOLDER, CREATE, "again.txt");
    apply(&mut tree.index, vec![again]);
    let id = tree.index.ids[&gone];
    let expected = format!(
        "{}{}again.txt",
        reference.table.path(reference.ids[&FIRST_FOLDER]).unwrap(),
        std::path::MAIN_SEPARATOR
    );
    assert_eq!(tree.index.table.path(id), Some(expected));
    assert_eq!(tree.index.table.size(id), Some(file_size(gone)));
    assert_eq!(listing(&tree.index)[&kept], listing(&reference)[&kept]);
}

#[test]
fn a_rename_split_across_a_compaction_lands_on_the_right_entry() {
    let mut tree = Tree::new(97, 30, 1_000);
    tree.delete(80, 0);
    let file = tree.live(&tree.files)[0];
    apply(&mut tree.index, vec![record(file, ROOT, OLD_NAME, "old")]);
    assert!(!tree.index.pending_renames().is_empty());
    let mut reference = compacted(&mut tree);
    let renamed = vec![record(file, FIRST_FOLDER + 1, NEW_NAME, "renamed.txt")];
    apply(&mut reference, renamed.clone());
    apply(&mut tree.index, renamed);
    assert!(tree.index.pending_renames().is_empty());
    let path = tree.index.table.path(tree.index.ids[&file]).unwrap();
    assert!(path.ends_with("renamed.txt"), "{path}");
    assert_eq!(listing(&tree.index), listing(&reference));
}

#[test]
fn the_batch_path_compacts_only_past_the_minimum_and_the_sparse_trigger() {
    let mut tree = Tree::new(5, 20, 1_999);
    tree.delete(100, 0);
    assert_eq!(tree.index.table.slots(), 2_020);
    let files = u128::from(COMPACT_MIN_SLOTS) + 10_000;
    let mut tree = Tree::new(11, 0, files);
    let slots = tree.index.table.slots();
    let delete = |range: std::ops::Range<u128>| {
        range
            .map(|file| record(file, ROOT, DELETE, "gone"))
            .collect()
    };
    apply(&mut tree.index, delete(FIRST_FILE..FIRST_FILE + 30_000));
    assert_eq!(
        tree.index.table.slots(),
        slots,
        "dense enough: no compaction"
    );
    let survivor = FIRST_FILE + files - 1;
    let reference = tree.index.table.path(tree.index.ids[&survivor]);
    let mut last: Vec<_> = delete(FIRST_FILE + 30_000..FIRST_FILE + 60_000);
    last.push(record(survivor, ROOT, OLD_NAME, "old"));
    apply(&mut tree.index, last);
    assert_dense_and_consistent(&tree.index);
    assert_eq!(tree.index.table.path(tree.index.ids[&survivor]), reference);
    apply(
        &mut tree.index,
        vec![record(survivor, ROOT, NEW_NAME, "moved.txt")],
    );
    let moved = tree.index.table.path(tree.index.ids[&survivor]).unwrap();
    assert_eq!(moved, format!("C:{}moved.txt", std::path::MAIN_SEPARATOR));
}

#[path = "compact_restore_tests.rs"]
mod restore;

#[path = "compact_order_tests.rs"]
mod order;
