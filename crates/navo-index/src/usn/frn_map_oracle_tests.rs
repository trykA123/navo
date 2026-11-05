use std::collections::HashMap;

use super::{EMPTY, FrnMap, MemoryParts, split};
use crate::heap_probe::{Probe, fail_allocations_from};

const WIDE: u128 = 1 << 100;
const RUNS: u64 = 50;
const OPERATIONS: usize = 2_000;

/// splitmix64: a fixed sequence per seed, so a failing run repeats.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

fn narrow(record: u64, seq: u16) -> u128 {
    u128::from((u64::from(seq) << 48) | record)
}

fn key(rng: &mut Rng) -> u128 {
    match rng.below(10) {
        0 => narrow((1 << 32) + rng.below(40), rng.below(2) as u16),
        1 => u128::from(u64::MAX),
        2 => WIDE | u128::from(rng.below(20)),
        3 => narrow(0xFFFF_FFFF_FFFF, rng.below(2) as u16),
        // Small records with a few `seq` values: same-record, other-seq pairs are common.
        _ => narrow(rng.below(2_100), rng.below(3) as u16),
    }
}

fn value(rng: &mut Rng) -> u32 {
    if rng.below(8) == 0 {
        EMPTY
    } else {
        rng.below(64) as u32
    }
}

fn sorted(map: &FrnMap) -> Vec<(u128, u32)> {
    let mut pairs: Vec<_> = map.iter().collect();
    pairs.sort_unstable();
    pairs
}

/// Length, sorted pairs and memory parts: what a failed insert must leave unchanged.
fn state(map: &FrnMap) -> (usize, Vec<(u128, u32)>, MemoryParts) {
    (map.len(), sorted(map), map.memory_parts())
}

fn agree(map: &FrnMap, oracle: &HashMap<u128, u32>, rng: &mut Rng) {
    assert_eq!(map.len(), oracle.len());
    let mut expected: Vec<_> = oracle.iter().map(|(&key, &id)| (key, id)).collect();
    expected.sort_unstable();
    assert_eq!(sorted(map), expected);
    for (key, id) in &expected {
        assert_eq!(map.get(key), Some(id), "{key:#x}");
    }
    for _ in 0..8 {
        let probe = key(rng);
        assert_eq!(map.get(&probe), oracle.get(&probe), "{probe:#x}");
    }
    for &reference in map.overflow.keys() {
        let (record, seq) = split(reference);
        assert_eq!(
            map.dense.get(record, seq),
            None,
            "{reference:#x} in two parts"
        );
    }
    assert!(map.wide.keys().all(|&key| u64::try_from(key).is_err()));
}

fn remap_table(rng: &mut Rng) -> Vec<Option<u32>> {
    (0..64)
        .map(|_| match rng.below(6) {
            0 => None,
            1 => Some(EMPTY),
            _ => Some(rng.below(64) as u32),
        })
        .collect()
}

/// One insert on both maps, through `try_insert` under `failing` when set; true when it failed.
fn insert(
    map: &mut FrnMap,
    oracle: &mut HashMap<u128, u32>,
    rng: &mut Rng,
    failing: Option<usize>,
) -> bool {
    let (reference, id) = (key(rng), value(rng));
    let Some(bytes) = failing else {
        let old = if rng.below(2) == 0 {
            map.insert(reference, id)
        } else {
            map.try_insert(reference, id).unwrap()
        };
        assert_eq!(old, oracle.insert(reference, id));
        return false;
    };
    let before = state(map);
    let result = {
        let _fail = fail_allocations_from(bytes);
        map.try_insert(reference, id)
    };
    match result {
        Ok(old) => assert_eq!(old, oracle.insert(reference, id)),
        Err(_) => assert_eq!(state(map), before, "{reference:#x}"),
    }
    result.is_err()
}

fn remap(map: &mut FrnMap, oracle: &mut HashMap<u128, u32>, rng: &mut Rng) {
    let table = remap_table(rng);
    let moved = |id: u32| table.get(id as usize).copied().flatten();
    map.remap(moved);
    oracle.retain(|_, id| match moved(*id) {
        Some(new) => {
            *id = new;
            true
        }
        None => false,
    });
}

/// One random operation on both maps; true when a failing insert returned `Err`.
fn step(
    map: &mut FrnMap,
    oracle: &mut HashMap<u128, u32>,
    rng: &mut Rng,
    failing: Option<usize>,
) -> bool {
    match rng.below(100) {
        0..40 => return insert(map, oracle, rng, failing),
        40..65 => {
            let reference = key(rng);
            assert_eq!(map.remove(&reference), oracle.remove(&reference));
        }
        65..80 => {
            let reference = key(rng);
            assert_eq!(map.get(&reference), oracle.get(&reference));
            assert_eq!(
                map.contains_key(&reference),
                oracle.contains_key(&reference)
            );
        }
        80..88 => {
            let mask = rng.next();
            let keep = |reference: u128, id: u32| (mask >> ((reference as u32 ^ id) % 64)) & 1 == 1;
            map.retain(|reference, &id| keep(reference, id));
            oracle.retain(|&reference, &mut id| keep(reference, id));
        }
        88..96 => remap(map, oracle, rng),
        96..99 => assert_eq!(map.iter().count(), oracle.len()),
        _ => {
            map.clear();
            oracle.clear();
        }
    }
    false
}

fn run(seed: u64, failing: Option<usize>) -> usize {
    let mut rng = Rng(seed);
    let mut map = FrnMap::new();
    let mut oracle = HashMap::new();
    let mut failures = 0;
    for _ in 0..OPERATIONS {
        failures += usize::from(step(&mut map, &mut oracle, &mut rng, failing));
        agree(&map, &oracle, &mut rng);
    }
    failures
}

#[test]
fn the_map_agrees_with_a_hash_map_over_seeded_runs() {
    for seed in 0..RUNS {
        run(seed, None);
    }
}

#[test]
fn every_failed_insert_leaves_the_map_unchanged() {
    for bytes in [4096, 2048, 1] {
        let failures: usize = (0..RUNS).map(|seed| run(seed + 1_000, Some(bytes))).sum();
        assert!(failures > 0, "no insert failed under {bytes} bytes");
    }
}

fn parts(map: &FrnMap, reference: u128) -> (bool, bool) {
    let narrow = u64::try_from(reference).unwrap();
    let (record, seq) = split(narrow);
    (
        map.dense.get(record, seq).is_some(),
        map.overflow.contains_key(&narrow),
    )
}

#[test]
fn another_seq_on_a_held_record_lives_in_overflow_until_it_is_updated() {
    let (first, second, third) = (narrow(300, 1), narrow(300, 2), narrow(300, 3));
    let mut map = FrnMap::new();
    map.insert(first, 1);
    map.insert(second, 2);
    assert_eq!(
        (parts(&map, first), parts(&map, second)),
        ((true, false), (false, true))
    );
    assert_eq!(map.remove(&first), Some(1));
    assert_eq!(map.get(&second), Some(&2));
    map.insert(third, 3);
    assert_eq!(parts(&map, third), (true, false));
    assert_eq!(map.insert(second, 4), Some(2));
    assert_eq!(parts(&map, second), (false, true));
    assert_eq!(map.get(&second), Some(&4));
    assert_eq!(map.len(), 2);
}

#[test]
fn an_empty_id_on_a_dense_key_moves_it_to_overflow() {
    let reference = narrow(400, 1);
    let mut map = FrnMap::new();
    map.insert(reference, 5);
    assert_eq!(parts(&map, reference), (true, false));
    assert_eq!(map.insert(reference, EMPTY), Some(5));
    assert_eq!(map.get(&reference), Some(&EMPTY));
    assert_eq!(parts(&map, reference), (false, true));
    assert_eq!(map.len(), 1);
}

#[test]
fn a_failed_move_to_overflow_keeps_the_old_id() {
    let reference = narrow(400, 1);
    let mut map = FrnMap::new();
    map.insert(reference, 5);
    let before = state(&map);
    let result = {
        let _fail = fail_allocations_from(1);
        map.try_insert(reference, EMPTY)
    };
    assert!(result.is_err());
    assert_eq!(map.get(&reference), Some(&5));
    assert_eq!(state(&map), before);
}

#[test]
fn an_aborting_insert_falls_back_to_overflow_when_a_page_cannot_be_allocated() {
    let mut map = FrnMap::new();
    for number in 0..20 {
        map.insert(narrow((1 << 32) + number, 0), number as u32);
    }
    for number in 0..10 {
        map.remove(&narrow((1 << 32) + number, 0));
    }
    let reference = narrow(5_000, 0);
    let pages = map.memory_parts().pages;
    let old = {
        let _fail = fail_allocations_from(2048);
        map.insert(reference, 7)
    };
    assert_eq!(old, None);
    assert_eq!(parts(&map, reference), (false, true));
    assert_eq!(map.iter().filter(|&(key, _)| key == reference).count(), 1);
    assert_eq!(map.memory_parts().pages, pages);
    assert_eq!(map.len(), 11);
}

#[test]
fn a_million_consecutive_references_cost_at_most_six_point_two_bytes() {
    const REFERENCES: u32 = 1_000_000;
    let probe = Probe::start();
    let mut map = FrnMap::with_capacity(REFERENCES as usize);
    for number in 0..REFERENCES {
        map.insert(u128::from(100 + number), number);
    }
    let held = usize::try_from(probe.read().retained).unwrap();
    let bytes = map.memory_bytes();
    assert_eq!(map.len(), REFERENCES as usize);
    assert!(bytes * 10 <= REFERENCES as usize * 62, "{bytes} bytes");
    assert_eq!(held, bytes);
}
