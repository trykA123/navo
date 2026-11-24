//! Realistic MFT fixture: seeded name lengths, record holes and a folder tree.
//!
//! The plan rides in the existing cursor field, so `MftFixture::new` and the record
//! helpers stay unchanged and enumeration asks this module for each chunk.

#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    reason = "the fixture reports its measured numbers and fails a broken record immediately"
)]

use super::{MftFixture, SyntheticRecord, append_record, stream_chunk};

/// First bit pattern in the cursor field; a normal cursor log starts at 0.
const PLAN_MAGIC: u64 = 0x414d_315f_5245_414c;
/// Root MFT reference used by every synthetic fixture.
const ROOT: u64 = 5;
/// First file record, past the NTFS metadata records.
const FIRST_RECORD: u64 = 16;
/// Records per streamed chunk: 192 maximum-size names stay under the 64 KiB buffer.
const CHUNK_RECORDS: u32 = 192;
/// One folder record followed by twelve file records.
const GROUP: u32 = 13;
/// Multiplier that spreads consecutive indices across the 64-bit state.
const STEP: u64 = 0x9e37_79b9_7f4a_7c15;
/// Lowercase letters, digits and safe punctuation for synthetic names.
const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789-_. ";

impl MftFixture {
    /// Builds a deterministic realistic MFT with `entries` records from `seed`; never fails.
    pub fn realistic(entries: u32, seed: u64) -> Self {
        Self {
            entries,
            journal_id: seed,
            enumeration_cursors: Some(build_plan(entries, seed)),
        }
    }
}

/// Returns the realistic chunk at `cursor`, or logs a normal cursor and returns `None`.
pub(super) fn step(fixture: &mut MftFixture, cursor: u64) -> Option<Vec<u8>> {
    let entries = fixture.entries;
    let seed = fixture.journal_id;
    let Some(plan) = &mut fixture.enumeration_cursors else {
        return None;
    };
    if plan.first() != Some(&PLAN_MAGIC) {
        plan.push(cursor);
        return None;
    }
    let start = u32::try_from(cursor).ok()?;
    if start >= entries {
        return None;
    }
    let end = start.saturating_add(CHUNK_RECORDS).min(entries);
    let plan = fixture.enumeration_cursors.as_ref()?;
    let bytes = stream_chunk(start, end, |buf, at| {
        let (file_id, parent_id) = plan_record(plan, at);
        let name = name(seed, at);
        append_record(
            buf,
            SyntheticRecord {
                file_id,
                parent_id,
                folder: at.is_multiple_of(GROUP),
                name: &name,
            },
        );
    });
    Some(bytes)
}

/// Returns the record number and parent record number stored for plan index `at`.
fn plan_record(plan: &[u64], at: u32) -> (u64, u64) {
    let base = 1 + 2 * at as usize;
    (
        plan.get(base).copied().unwrap_or_default(),
        plan.get(base + 1).copied().unwrap_or_default(),
    )
}

/// Packs `entries` record numbers and parents behind [`PLAN_MAGIC`].
fn build_plan(entries: u32, seed: u64) -> Vec<u64> {
    let mut plan = Vec::with_capacity(1 + 2 * entries as usize);
    plan.push(PLAN_MAGIC);
    let mut number = FIRST_RECORD;
    let mut folders: Vec<u64> = Vec::new();
    for at in 0..entries {
        if at > 0 {
            number += gap(seed, at);
        }
        let folder = at.is_multiple_of(GROUP);
        let parent = if folder {
            let depth = at / GROUP % 8;
            let parent = if depth == 0 {
                ROOT
            } else {
                folders.last().copied().unwrap_or(ROOT)
            };
            folders.push(number);
            parent
        } else {
            folders.last().copied().unwrap_or(ROOT)
        };
        plan.push(number);
        plan.push(parent);
    }
    plan
}

/// Returns the record-number step for plan index `at`: 1 at 60 %, else 2 or 3.
fn gap(seed: u64, at: u32) -> u64 {
    match mix(seed ^ u64::from(at).wrapping_mul(STEP)) % 15 {
        0..=8 => 1,
        9..=10 => 2,
        _ => 3,
    }
}

/// Returns `(byte length, has one two-byte character)` for plan index `at`.
fn name_shape(seed: u64, at: u32) -> (usize, bool) {
    let hash = mix(seed ^ u64::from(at).wrapping_mul(STEP));
    let (low, span) = match hash % 100 {
        0..=19 => (6, 7),
        20..=69 => (13, 20),
        70..=94 => (33, 32),
        _ => (65, 56),
    };
    let length = low + (hash >> 8) as usize % span;
    (length, (hash >> 16).is_multiple_of(20))
}

/// Builds the deterministic name for plan index `at` at its target byte length.
fn name(seed: u64, at: u32) -> String {
    let (length, unicode) = name_shape(seed, at);
    let mut state = mix(seed ^ (u64::from(at) << 32) ^ length as u64);
    let mut out = String::with_capacity(length);
    for _ in 0..length - usize::from(unicode) {
        state = mix(state);
        out.push(ALPHABET[state as usize % ALPHABET.len()] as char);
    }
    if unicode {
        let at = (mix(state) as usize) % (out.len() + 1);
        out.insert(at, 'é');
    }
    out
}

/// splitmix64: mixes `state` into a well-distributed 64-bit value.
fn mix(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ntfs::{Flags, NtfsIndex};
    use crate::table::NO_PARENT;

    /// Builds the index for `entries` records from `seed`.
    fn index(entries: u32, seed: u64) -> NtfsIndex {
        MftFixture::realistic(entries, seed).read().index
    }

    /// Returns sorted `(reference, name, parent)` rows for comparing two builds.
    fn rows(index: &NtfsIndex) -> Vec<(u128, String, u32)> {
        let mut rows: Vec<_> = index
            .ids
            .iter()
            .map(|(frn, id)| {
                (
                    frn,
                    index.table.name(id).unwrap_or_default().to_string(),
                    index.table.parent(id).unwrap_or(NO_PARENT),
                )
            })
            .collect();
        rows.sort_unstable();
        rows
    }

    #[test]
    fn realistic_fixture_name_lengths_match_the_mix() {
        let index = index(100_000, 7);
        let mut bytes = 0usize;
        let mut names = 0usize;
        for (frn, id) in index.ids.iter() {
            if frn == 5 {
                continue;
            }
            bytes += index.table.name(id).unwrap_or_default().len();
            names += 1;
        }
        let mean = bytes as f64 / names as f64;
        println!("AM1 mean name length: {mean:.2} bytes over {names} names");
        assert!(
            (28.0..=32.0).contains(&mean),
            "mean name length {mean:.2} bytes is outside 28-32"
        );
    }

    #[test]
    fn realistic_fixture_has_forty_percent_holes() {
        let index = index(100_000, 7);
        let numbers: Vec<u64> = index
            .ids
            .iter()
            .filter(|(frn, _)| *frn != 5)
            .map(|(frn, _)| u64::try_from(frn).unwrap_or_default())
            .collect();
        let max = numbers.iter().copied().max().unwrap_or_default();
        let holes = max as usize - numbers.len();
        let share = holes as f64 / max as f64;
        println!("AM1 hole share: {share:.3} ({holes} of {max})");
        assert!(
            (0.35..=0.45).contains(&share),
            "hole share {share:.3} is outside 0.35-0.45"
        );
    }

    #[test]
    fn realistic_fixture_is_deterministic() {
        assert_eq!(rows(&index(2_000, 7)), rows(&index(2_000, 7)));
        assert_ne!(rows(&index(2_000, 7)), rows(&index(2_000, 8)));
    }

    /// Returns the folder depth of `id` and asserts that every parent up to `root` is a folder.
    fn folder_depth(index: &NtfsIndex, root: u32, frn: u128, id: u32) -> u32 {
        let mut depth = 0;
        let mut current = id;
        while let Some(parent) = index.table.parent(current) {
            if parent == root {
                return depth;
            }
            assert!(
                index
                    .table
                    .flags(parent)
                    .is_some_and(|flags| flags.contains(Flags::FOLDER)),
                "record {frn} has non-folder parent {parent}"
            );
            depth += 1;
            assert!(depth <= 8, "record {frn} is deeper than 8 folders");
            current = parent;
        }
        assert!(current == root, "record {frn} does not reach the root");
        depth
    }

    #[test]
    fn realistic_fixture_builds_an_index() {
        let index = index(10_000, 11);
        let root = index.ids.get(&5).copied().unwrap_or_default();
        assert_eq!(index.ids.len(), 10_001);
        let mut max_depth = 0;
        for (frn, id) in index.ids.iter() {
            if frn != 5 {
                max_depth = max_depth.max(folder_depth(&index, root, frn, id));
            }
        }
        assert!(max_depth <= 8, "max folder depth {max_depth} exceeds 8");
    }
}
