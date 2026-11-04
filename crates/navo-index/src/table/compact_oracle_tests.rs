#![allow(
    clippy::print_stdout,
    reason = "the peak test prints the measured number"
)]

use std::num::NonZeroU32;

use super::Remap;
use crate::heap_probe::Probe;
use crate::table::{Flags, NAME_END, NO_PARENT, NameTable};

const ORACLE_SLOTS: u32 = 2_000;
const PEAK_SLOTS: u32 = 300_000;
const PEAK_PER_LIVE: usize = 40;

/// The single-pass compaction before the column-by-column rewrite, kept as the oracle.
fn compact_reference(table: &mut NameTable) -> Remap {
    let slots = table.slots();
    let mut map = Remap(vec![None; slots as usize]);
    let mut next = 0u32;
    for id in 0..slots {
        if table.is_live(id) {
            map.0[id as usize] = next.checked_add(1).and_then(NonZeroU32::new);
            next += 1;
        }
    }
    let mut fresh = NameTable::with_capacity(next as usize, table.names.len());
    for id in 0..slots {
        if map.get(id).is_none() {
            continue;
        }
        let parent = map.get(table.parents[id as usize]).unwrap_or(NO_PARENT);
        fresh.offsets.push(fresh.names.len() as u32);
        fresh.names.push_str(table.name_unchecked(id));
        fresh.names.push(NAME_END);
        fresh.parents.push(parent);
        fresh.flags.push(table.flags[id as usize]);
        fresh.bytes.push(table.bytes[id as usize]);
        if let Ok(at) = table.counts.binary_search_by_key(&id, |&(slot, _)| slot) {
            fresh.counts.push((fresh.slots() - 1, table.counts[at].1));
        }
    }
    fresh.live = next as usize;
    match table.children.remapped(&map, &fresh.parents) {
        Some(children) => fresh.children = children,
        None => {
            tracing::error!("child lists disagree with parents during compaction; rebuilt");
            fresh.relink();
        }
    }
    fresh.shrink_to_fit();
    *table = fresh;
    map
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

fn live_folder(t: &NameTable, folders: &[u32], rng: &mut Rng) -> Option<u32> {
    let folder = folders[rng.below(folders.len() as u32) as usize];
    t.flags(folder).is_some().then_some(folder)
}

fn seeded(seed: u64) -> NameTable {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut t = NameTable::new();
    let mut folders = vec![t.push(NO_PARENT, "C:", Flags::FOLDER).unwrap()];
    while t.slots() < ORACLE_SLOTS {
        let pick = rng.below(t.slots());
        let name = format!("n{}", rng.below(5_000));
        match rng.below(20) {
            0..=3 => {
                if let Some(parent) = live_folder(&t, &folders, &mut rng) {
                    folders.push(t.push(parent, &name, Flags::FOLDER).unwrap());
                }
            }
            4 => {
                let parent = t.slots() + 1 + rng.below(3_000);
                t.push(parent, &name, Flags::NONE).unwrap();
            }
            5 | 6 => {
                if let Some(parent) = live_folder(&t, &folders, &mut rng) {
                    let _ = t.rename(pick, parent, &name);
                }
            }
            7 if pick != 0 => {
                t.remove(pick);
            }
            8..=10 => {
                let _ = t.set_size(pick, u64::from(rng.below(1 << 20)));
            }
            _ => {
                if let Some(parent) = live_folder(&t, &folders, &mut rng) {
                    t.push(parent, &name, Flags::NONE).unwrap();
                }
            }
        }
    }
    t
}

fn assert_same(seed: u64, expected: &NameTable, actual: &NameTable) {
    assert_eq!(expected.raw_parts(), actual.raw_parts(), "seed {seed}");
    let totals = |t: &NameTable| {
        let (bytes, counts) = t.stored_totals();
        (bytes.to_vec(), counts.collect::<Vec<_>>())
    };
    assert_eq!(totals(expected), totals(actual), "seed {seed}");
    assert_eq!(expected.len(), actual.len(), "seed {seed}");
    assert_eq!(expected.slots(), actual.slots(), "seed {seed}");
    for id in 0..expected.slots() {
        assert_eq!(
            expected.child_names(id),
            actual.child_names(id),
            "seed {seed} id {id}"
        );
    }
    assert_eq!(
        expected.memory_parts(),
        actual.memory_parts(),
        "seed {seed}"
    );
}

#[test]
fn compaction_matches_the_reference_on_seeded_tables() {
    for seed in 1..=50 {
        let mut expected = seeded(seed);
        let mut actual = expected.clone();
        let slots = expected.slots();
        let expected_map = compact_reference(&mut expected);
        let actual_map = actual.compact();
        for old in (0..=slots).chain([u32::MAX]) {
            assert_eq!(expected_map.get(old), actual_map.get(old), "seed {seed}");
        }
        assert_same(seed, &expected, &actual);
    }
}

/// Root, 1,000 folders, then files under them; odd files and ten folders removed.
fn sparse_table() -> NameTable {
    let mut t = NameTable::new();
    let root = t.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    for folder in 1..=1_000 {
        t.push(root, &format!("folder {folder}"), Flags::FOLDER)
            .unwrap();
    }
    while t.slots() < PEAK_SLOTS {
        let id = t.slots();
        t.push(id % 1_000 + 1, &format!("file-{id}.txt"), Flags::NONE)
            .unwrap();
    }
    for id in (1_001..PEAK_SLOTS).step_by(2) {
        t.remove(id);
    }
    for folder in 1..=10 {
        t.remove(folder);
    }
    t.shrink_to_fit();
    t
}

#[test]
fn compaction_peak_stays_within_40_bytes_per_live_entry() {
    let mut t = sparse_table();
    assert!(2 * t.len() < t.slots() as usize);
    let probe = Probe::start();
    let map = t.compact();
    let measured = probe.read();
    let per_live = measured.peak / t.len();
    println!(
        "RAM compact_columns: peak {} KiB, per-live {per_live} B, live {}",
        measured.peak / 1024,
        t.len()
    );
    assert_eq!(t.slots() as usize, t.len());
    assert!(
        per_live <= PEAK_PER_LIVE,
        "compaction peak {per_live} B/live"
    );
    drop(map);
}
