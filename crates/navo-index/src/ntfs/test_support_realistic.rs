//! Realistic MFT fixture: seeded name lengths, record holes and a folder tree.
//!
//! The fixture keeps its seed and record plan in its own `realistic` field, so the
//! journal id and the cursor log keep their real meaning.

#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    reason = "the fixture reports its measured numbers and fails a broken record immediately"
)]

use super::{MftFixture, SyntheticRecord, append_record, stream_chunk};

/// Journal id every realistic fixture reports; the seed shapes only the records.
const JOURNAL_ID: u64 = 1;
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

/// The seed and the `(record number, parent record number)` pairs of a realistic fixture.
pub(super) struct RealisticPlan {
    seed: u64,
    records: Vec<(u64, u64)>,
}

impl MftFixture {
    /// Builds a deterministic realistic MFT with `entries` records from `seed`; never fails.
    pub fn realistic(entries: u32, seed: u64) -> Self {
        Self {
            entries,
            journal_id: JOURNAL_ID,
            enumeration_cursors: None,
            realistic: Some(RealisticPlan {
                seed,
                records: build_plan(entries, seed),
            }),
        }
    }
}

impl RealisticPlan {
    /// Returns the chunk of plan records from `cursor`, or `None` past the last of `entries`.
    pub(super) fn chunk(&self, entries: u32, cursor: u64) -> Option<Vec<u8>> {
        let start = u32::try_from(cursor).ok()?;
        if start >= entries {
            return None;
        }
        let end = start.saturating_add(CHUNK_RECORDS).min(entries);
        let bytes = stream_chunk(start, end, |buf, at| {
            let (file_id, parent_id) = self.records.get(at as usize).copied().unwrap_or_default();
            let name = name(self.seed, at);
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
}

/// Lays out `entries` record numbers with seeded holes and their parent folders.
fn build_plan(entries: u32, seed: u64) -> Vec<(u64, u64)> {
    let mut plan = Vec::with_capacity(entries as usize);
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
        plan.push((number, parent));
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
    use crate::ntfs::volume_io::VolumeIo;
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
    fn realistic_fixture_keeps_a_real_journal_id() {
        let journal = |seed| MftFixture::realistic(100, seed).query_journal().unwrap();
        assert_eq!(journal(7), journal(8));
    }

    #[test]
    fn realistic_fixture_logs_real_cursors() {
        let (enumeration, cursors) = MftFixture::realistic(2_000, 7).read_with_cursors();
        assert_eq!(cursors.first(), Some(&0));
        assert_eq!(rows(&enumeration.index), rows(&index(2_000, 7)));
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
