use std::collections::{HashMap, TryReserveError};
use std::ops::Index;

use super::frn_pages::{DensePages, EMPTY, Placed};

const RECORD_MASK: u64 = 0xFFFF_FFFF_FFFF;

/// Maps a file reference to its table entry id.
/// Narrow references sit in dense pages by MFT record (about 6 B each); the rest go to `overflow`
/// (narrow) or `wide` (ReFS-style). Every key lives in exactly one of the three.
#[derive(Debug, Clone, Default)]
pub struct FrnMap {
    dense: DensePages,
    overflow: HashMap<u64, u32>,
    wide: HashMap<u128, u32>,
}

/// An NTFS reference: the low 48 bits are the MFT record, the high 16 its sequence number.
fn split(reference: u64) -> (u64, u16) {
    (reference & RECORD_MASK, (reference >> 48) as u16)
}

fn join(record: u64, seq: u16) -> u128 {
    u128::from((u64::from(seq) << 48) | record)
}

impl FrnMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserves nothing: dense pages are sized by record numbers, not by a count.
    pub fn with_capacity(_references: usize) -> Self {
        Self::default()
    }

    /// Reserves nothing, for the reason given on [`Self::with_capacity`].
    pub fn try_reserve(&mut self, _references: usize) -> Result<(), TryReserveError> {
        Ok(())
    }

    /// Inserts like [`Self::insert`], but a failed allocation is an `Err` that leaves the map unchanged.
    pub fn try_insert(&mut self, reference: u128, id: u32) -> Result<Option<u32>, TryReserveError> {
        let Ok(narrow) = u64::try_from(reference) else {
            self.wide.try_reserve(1)?;
            return Ok(self.wide.insert(reference, id));
        };
        if let Some(held) = self.overflow.get_mut(&narrow) {
            return Ok(Some(std::mem::replace(held, id)));
        }
        let (record, seq) = split(narrow);
        if id == EMPTY && self.dense.get(record, seq).is_some() {
            self.overflow.try_reserve(1)?;
            let old = self.dense.remove(record, seq);
            self.overflow.insert(narrow, id);
            return Ok(old);
        }
        match self.dense.try_insert(record, seq, id)? {
            Placed::Inserted => Ok(None),
            Placed::Updated(old) => Ok(Some(old)),
            Placed::Refused => {
                self.overflow.try_reserve(1)?;
                Ok(self.overflow.insert(narrow, id))
            }
        }
    }

    pub fn len(&self) -> usize {
        self.dense.len() + self.overflow.len() + self.wide.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, reference: &u128) -> Option<&u32> {
        let Ok(narrow) = u64::try_from(*reference) else {
            return self.wide.get(reference);
        };
        let (record, seq) = split(narrow);
        self.dense
            .get_ref(record, seq)
            .or_else(|| self.overflow.get(&narrow))
    }

    pub fn contains_key(&self, reference: &u128) -> bool {
        self.get(reference).is_some()
    }

    /// Inserts with std's aborting growth when a page or a bucket cannot be allocated.
    pub fn insert(&mut self, reference: u128, id: u32) -> Option<u32> {
        if let Ok(old) = self.try_insert(reference, id) {
            return old;
        }
        let Ok(narrow) = u64::try_from(reference) else {
            return self.wide.insert(reference, id);
        };
        let (record, seq) = split(narrow);
        // The failed step changed nothing; the key is in dense only on the move to `overflow`.
        let old = self.dense.remove(record, seq);
        self.overflow.insert(narrow, id).or(old)
    }

    pub fn remove(&mut self, reference: &u128) -> Option<u32> {
        let Ok(narrow) = u64::try_from(*reference) else {
            return self.wide.remove(reference);
        };
        let (record, seq) = split(narrow);
        self.dense
            .remove(record, seq)
            .or_else(|| self.overflow.remove(&narrow))
    }

    pub fn retain(&mut self, mut keep: impl FnMut(u128, &u32) -> bool) {
        self.dense
            .retain(|record, seq, id| keep(join(record, seq), &id));
        self.overflow.retain(|&key, id| keep(u128::from(key), id));
        self.wide.retain(|&key, id| keep(key, id));
    }

    /// Rewrites every id through `new_id` and drops references it rejects. Capacity is kept;
    /// a dense pair whose new id is `EMPTY` moves to `overflow`.
    pub fn remap(&mut self, mut new_id: impl FnMut(u32) -> Option<u32>) {
        let moved = self.dense.remap(&mut new_id);
        self.overflow.retain(|_, id| remapped(&mut new_id, id));
        self.wide.retain(|_, id| remapped(&mut new_id, id));
        for (record, seq, id) in moved {
            self.overflow.insert((u64::from(seq) << 48) | record, id);
        }
    }

    pub fn clear(&mut self) {
        self.dense.clear();
        self.overflow.clear();
        self.wide.clear();
    }

    pub fn shrink_to_fit(&mut self) {
        self.dense.shrink_to_fit();
        self.overflow.shrink_to_fit();
        self.wide.shrink_to_fit();
    }

    /// Dense pairs in record order, then `overflow`, then `wide`.
    pub fn iter(&self) -> impl Iterator<Item = (u128, u32)> + '_ {
        let dense = self
            .dense
            .iter()
            .map(|(record, seq, id)| (join(record, seq), id));
        let overflow = self
            .overflow
            .iter()
            .map(|(&key, &id)| (u128::from(key), id));
        dense
            .chain(overflow)
            .chain(self.wide.iter().map(|(&key, &id)| (key, id)))
    }

    pub fn values(&self) -> impl Iterator<Item = &u32> + '_ {
        self.dense
            .values()
            .chain(self.overflow.values())
            .chain(self.wide.values())
    }

    /// Heap bytes: page directory, pages, and one bucket plus one control byte per hash slot.
    pub fn memory_bytes(&self) -> usize {
        let (directory, pages) = self.dense.memory_parts();
        directory + pages + self.table_bytes()
    }

    fn table_bytes(&self) -> usize {
        table_bytes::<(u64, u32)>(self.overflow.capacity())
            + table_bytes::<(u128, u32)>(self.wide.capacity())
    }

    #[cfg(test)]
    fn memory_parts(&self) -> MemoryParts {
        let (directory, pages) = self.dense.memory_parts();
        MemoryParts {
            directory,
            pages,
            tables: self.table_bytes(),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemoryParts {
    directory: usize,
    pages: usize,
    tables: usize,
}

/// Equal when both hold the same reference-to-id pairs, wherever each pair is stored.
impl PartialEq for FrnMap {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .all(|(reference, id)| other.get(&reference) == Some(&id))
    }
}

impl Eq for FrnMap {}

fn remapped(new_id: &mut impl FnMut(u32) -> Option<u32>, id: &mut u32) -> bool {
    match new_id(*id) {
        Some(moved) => {
            *id = moved;
            true
        }
        None => false,
    }
}

/// Allocated bytes of a hash table holding `capacity` usable entries (7/8 load, power-of-two buckets, minimum 4).
fn table_bytes<T>(capacity: usize) -> usize {
    if capacity == 0 {
        return 0;
    }
    let buckets = if capacity < 8 {
        if capacity < 4 { 4 } else { 8 }
    } else {
        (capacity * 8 / 7).next_power_of_two()
    };
    buckets * (size_of::<T>() + 1)
}

impl Index<&u128> for FrnMap {
    type Output = u32;

    #[expect(
        clippy::panic,
        reason = "indexing a missing reference is a caller bug, like HashMap's Index"
    )]
    fn index(&self, reference: &u128) -> &u32 {
        self.get(reference)
            .unwrap_or_else(|| panic!("file reference {reference} is not in the map"))
    }
}

impl FromIterator<(u128, u32)> for FrnMap {
    fn from_iter<I: IntoIterator<Item = (u128, u32)>>(pairs: I) -> Self {
        let mut map = Self::new();
        for (reference, id) in pairs {
            map.insert(reference, id);
        }
        map
    }
}

impl<const N: usize> From<[(u128, u32); N]> for FrnMap {
    fn from(pairs: [(u128, u32); N]) -> Self {
        pairs.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::FrnMap;

    const WIDE: u128 = (1 << 100) | 8;

    #[test]
    fn memory_counts_pages_directory_and_buckets() {
        let mut map = FrnMap::with_capacity(100);
        assert_eq!(map.memory_bytes(), 0);
        map.try_insert(WIDE, 0).unwrap();
        // 3 wide entries fit in 4 buckets.
        assert_eq!(map.memory_bytes(), 4 * 33);
        map.try_insert(5, 1).unwrap();
        assert_eq!(map.memory_bytes(), 4 * 33 + 40 + 6 * 1024);
        map.try_insert(u128::from(u64::MAX), 2).unwrap();
        assert_eq!(map.memory_bytes(), 4 * 33 + 40 + 6 * 1024 + 4 * 17);
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn narrow_and_wide_references_are_found_inserted_and_removed() {
        let mut map: FrnMap = [(5, 0), (WIDE, 1), (u128::from(u64::MAX), 2)].into();
        assert_eq!(map.len(), 3);
        assert_eq!(map[&WIDE], 1);
        assert_eq!(map.get(&u128::from(u64::MAX)), Some(&2));
        assert_eq!(map.get(&(u128::from(u64::MAX) + 1)), None);
        assert_eq!(map.insert(WIDE, 4), Some(1));
        assert_eq!(map.remove(&5), Some(0));
        assert!(!map.contains_key(&5));
        let mut pairs: Vec<_> = map.iter().collect();
        pairs.sort_unstable();
        assert_eq!(pairs, [(u128::from(u64::MAX), 2), (WIDE, 4)]);
    }

    #[test]
    fn remap_rewrites_both_widths_through_a_lookup_and_keeps_memory() {
        let mut map: FrnMap = [(5, 0), (WIDE, 3), (9, 1), (10, 2), (11, 7)].into();
        let bytes = map.memory_bytes();
        let moved = [Some(0), None, Some(1), Some(2)];
        map.remap(|id| moved.get(id as usize).copied().flatten());
        assert_eq!(map, [(5, 0), (WIDE, 2), (10, 1)].into());
        assert_eq!((map.len(), map.get(&9), map.get(&10)), (3, None, Some(&1)));
        assert_eq!(map.memory_bytes(), bytes);
    }

    #[test]
    fn retain_sees_both_widths() {
        let mut map: FrnMap = [(5, 0), (WIDE, 1), (9, 2)].into();
        map.retain(|reference, &id| reference == WIDE || id == 2);
        assert_eq!(map, [(WIDE, 1), (9, 2)].into());
    }
}

#[cfg(test)]
#[path = "frn_map_oracle_tests.rs"]
mod oracle_tests;
