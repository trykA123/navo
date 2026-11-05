#![allow(
    clippy::print_stdout,
    reason = "the peak test prints the measured number"
)]

use super::*;
use crate::heap_probe::Probe;
use crate::table::tests::tree;
use crate::table::{NO_PARENT, StoredFlags};

#[test]
fn totals_follow_insert_resize_move_and_subtree_removal() {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let old = table.push(root, "old", Flags::FOLDER).unwrap();
    let new = table.push(root, "new", Flags::FOLDER).unwrap();
    let branch = table.push(old, "branch", Flags::FOLDER).unwrap();
    let leaf = table.push(branch, "leaf", Flags::FOLDER).unwrap();
    let file = table.push(leaf, "file", Flags::NONE).unwrap();
    table.set_size(file, 100).unwrap();
    assert_eq!(
        table.totals(root),
        Some(Totals {
            bytes: 100,
            files: 1,
            folders: 4
        })
    );
    table.rename(branch, new, "moved").unwrap();
    assert_eq!(table.totals(old), Some(Totals::default()));
    assert_eq!(
        table.totals(new),
        Some(Totals {
            bytes: 100,
            files: 1,
            folders: 2
        })
    );
    assert_eq!(table.find_folder("c:/NEW/moved/leaf/"), Some(leaf));
    table.set_size(file, 25).unwrap();
    assert_eq!(table.totals(root).unwrap().bytes, 25);
    assert_eq!(table.totals(new).unwrap().bytes, 25);
    table.set_size(file, 300).unwrap();
    assert_eq!(table.totals(root).unwrap().bytes, 300);
    assert_eq!(table.remove(branch), 3);
    assert_eq!(table.totals(new), Some(Totals::default()));
    assert_eq!(
        table.totals(root),
        Some(Totals {
            bytes: 0,
            files: 0,
            folders: 2
        })
    );
    assert_eq!(table.size(file), None);
}

#[test]
fn late_parents_and_compaction_keep_subtree_totals() {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let file = table.push(2, "file", Flags::NONE).unwrap();
    table.set_size(file, 10).unwrap();
    let folder = table.push(root, "folder", Flags::FOLDER).unwrap();
    assert_eq!(
        table.totals(root),
        Some(Totals {
            bytes: 10,
            files: 1,
            folders: 1
        })
    );
    table.compact();
    assert_eq!(
        table.totals(folder),
        Some(Totals {
            bytes: 10,
            files: 1,
            folders: 0
        })
    );
    table.set_size(file, 20).unwrap();
    assert_eq!(table.totals(root).unwrap().bytes, 20);
}

#[test]
fn folder_lookup_uses_unicode_case_folding() {
    let mut table = NameTable::new();
    table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let folder = table.push(0, "Ä", Flags::FOLDER).unwrap();
    assert_eq!(table.find_folder("c:/ä"), Some(folder));
}

#[test]
fn files_do_not_allocate_folder_counts() {
    let mut table = NameTable::new();
    table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    for _ in 0..1000 {
        table.push(0, "file", Flags::NONE).unwrap();
    }
    assert_eq!(table.counts.len(), 1);
    let folder = table.push(0, "folder", Flags::FOLDER).unwrap();
    assert_eq!(table.counts.len(), 2);
    assert_eq!(table.totals(0).unwrap().files, 1000);
    table.remove(folder);
    table.compact();
    assert_eq!(table.counts.len(), 1);
}

const ORACLE_SLOTS: u32 = 2_000;
const PEAK_SLOTS: u32 = 200_000;

/// Load totals before the scratch-free walk, kept as the oracle.
fn rebuild_reference(t: &mut NameTable) {
    t.counts = t.counted_folders().map(|id| (id, [0; 2])).collect();
    for id in 0..t.slots() {
        if t.flags(id)
            .is_some_and(|flags| flags.contains(Flags::FOLDER))
        {
            t.bytes[id as usize] = 0;
        }
    }
    let mut remaining: Vec<_> = (0..t.slots()).map(|id| t.children.of(id).count()).collect();
    let mut ready: Vec<_> = t
        .live_ids()
        .filter(|&id| remaining[id as usize] == 0)
        .collect();
    while let Some(id) = ready.pop() {
        let Some(parent) = t.parent(id) else {
            continue;
        };
        let totals = t.contribution(id);
        let at = parent as usize;
        t.bytes[at] = t.bytes[at].saturating_add(totals.bytes);
        if let Some(counts) = t.folder_counts_mut(parent) {
            counts[0] += totals.files;
            counts[1] += totals.folders;
        }
        remaining[at] -= 1;
        if remaining[at] == 0 {
            ready.push(parent);
        }
    }
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % u64::from(bound)) as u32
    }
}

/// Valid tables only: pushes, renames across parents, subtree removals and resizes.
fn seeded(seed: u64) -> NameTable {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut t = NameTable::new();
    let mut folders = vec![t.push(NO_PARENT, "C:", Flags::FOLDER).unwrap()];
    while t.slots() < ORACLE_SLOTS {
        let pick = 1 + rng.below(t.slots());
        let parent = folders[rng.below(folders.len() as u32) as usize];
        if t.flags(parent).is_none() {
            continue;
        }
        let name = format!("n{}", rng.below(5_000));
        match rng.below(10) {
            0..=2 => folders.push(t.push(parent, &name, Flags::FOLDER).unwrap()),
            3 => {
                let _ = t.rename(pick, parent, &name);
            }
            4 => {
                t.remove(pick);
            }
            5 | 6 => {
                let _ = t.set_size(pick, u64::from(rng.below(1 << 20)));
            }
            _ => {
                let file = t.push(parent, &name, Flags::NONE).unwrap();
                t.set_size(file, u64::from(rng.below(1 << 20))).unwrap();
            }
        }
    }
    t
}

fn totals_of(t: &NameTable) -> (Vec<u64>, Vec<[u32; 2]>) {
    let (bytes, counts) = t.stored_totals();
    (bytes.to_vec(), counts.collect())
}

/// Wrong folder sums, so only a rebuild can bring the right ones back.
fn scramble(t: &mut NameTable) {
    for id in t.counted_folders().collect::<Vec<_>>() {
        t.bytes[id as usize] = 7;
    }
    for (_, counts) in &mut t.counts {
        *counts = [9, 9];
    }
}

#[test]
fn rebuilt_totals_match_the_reference_on_seeded_tables() {
    for seed in 1..=50 {
        let built = seeded(seed);
        assert!(built.valid_structure(), "seed {seed}");
        let mut relinked = built.clone();
        relinked.relink();
        for mut actual in [built.clone(), relinked] {
            let mut expected = actual.clone();
            rebuild_reference(&mut expected);
            scramble(&mut actual);
            assert!(actual.rebuild_totals(), "seed {seed}");
            assert_eq!(totals_of(&expected), totals_of(&actual), "seed {seed}");
            assert_eq!(totals_of(&built), totals_of(&actual), "seed {seed}");
        }
    }
}

/// Every fourth slot a folder under an earlier folder; files sized by id.
fn quarter_folders() -> NameTable {
    let mut t = NameTable::new();
    t.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    for id in 1..PEAK_SLOTS {
        if id % 4 == 0 {
            t.push(id / 8 * 4, &format!("d{id}"), Flags::FOLDER)
                .unwrap();
        } else {
            t.push(id / 4 * 4, &format!("f{id}"), Flags::NONE).unwrap();
            t.set_size(id, u64::from(id)).unwrap();
        }
    }
    t
}

#[test]
fn rebuilding_totals_needs_only_the_folder_counts() {
    let mut t = quarter_folders();
    let expected = totals_of(&t);
    let folders = t.counts.len();
    t.counts = Vec::new();
    t.shrink_to_fit();
    let probe = Probe::start();
    assert!(t.rebuild_totals());
    let measured = probe.read();
    println!(
        "RAM rebuild_totals: peak {} KiB, folders {folders}",
        measured.peak / 1024
    );
    assert!(
        measured.peak <= 12 * folders + 64 * 1024,
        "rebuild peak {} B for {folders} folders",
        measured.peak
    );
    assert_eq!(totals_of(&t), expected);
}

/// Reloads `t` through `from_raw_parts` after `edit` changes its parents or flags.
fn reload(
    t: &NameTable,
    edit: impl FnOnce(&mut Vec<u32>, &mut Vec<StoredFlags>),
) -> Option<NameTable> {
    let (names, offsets, parents, flags) = t.raw_parts();
    let (mut parents, mut flags) = (parents.to_vec(), flags.to_vec());
    edit(&mut parents, &mut flags);
    let (bytes, counts) = totals_of(t);
    NameTable::from_raw_parts(
        names.to_owned(),
        (offsets.to_vec(), parents, flags),
        (bytes, counts),
    )
}

#[test]
fn an_empty_and_an_all_deleted_table_load() {
    let empty =
        NameTable::from_raw_parts(String::new(), (vec![], vec![], vec![]), (vec![], vec![]))
            .unwrap();
    assert_eq!((empty.slots(), empty.len()), (0, 0));
    let (t, _) = tree();
    let (names, offsets, parents, flags) = t.raw_parts();
    let flags = flags.iter().map(|bits| bits | Flags::DELETED).collect();
    let slots = offsets.len();
    let gone = NameTable::from_raw_parts(
        names.to_owned(),
        (offsets.to_vec(), parents.to_vec(), flags),
        (vec![0; slots], vec![[0; 2]; slots]),
    )
    .unwrap();
    assert_eq!((gone.slots(), gone.len()), (t.slots(), 0));
}

#[test]
fn a_parent_cycle_a_child_of_a_deleted_folder_and_a_child_of_a_file_do_not_load() {
    let (t, [_, users, ann, notes, docs]) = tree();
    assert!(reload(&t, |_, _| {}).is_some());
    let cycle = reload(&t, |parents, _| parents[users as usize] = ann);
    assert!(cycle.is_none());
    let orphan = reload(&t, |_, flags| flags[ann as usize] |= Flags::DELETED);
    assert!(orphan.is_none());
    let under_file = reload(&t, |parents, _| parents[docs as usize] = notes);
    assert!(under_file.is_none());
}

#[test]
fn a_self_looping_link_and_a_missing_child_fail_the_rebuild() {
    let (mut looped, [_, _, ann, notes, docs]) = tree();
    assert_eq!(looped.children.of(ann).collect::<Vec<_>>(), [docs, notes]);
    looped.children.set_next(docs, docs);
    assert!(!looped.rebuild_totals());
    let (mut missing, _) = tree();
    missing.children.detach(notes, ann);
    assert!(!missing.rebuild_totals());
}
