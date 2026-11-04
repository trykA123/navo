use std::collections::{HashMap, TryReserveError};
use std::ops::Index;

/// Maps a file reference to its table entry id.
/// NTFS references fit in 64 bits, so their buckets take 16 bytes instead of 32 for a `u128` key;
/// wider references (ReFS-style) go to a small second map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrnMap {
    narrow: HashMap<u64, u32>,
    wide: HashMap<u128, u32>,
}

impl FrnMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(references: usize) -> Self {
        Self {
            narrow: HashMap::with_capacity(references),
            wide: HashMap::new(),
        }
    }

    pub fn try_reserve(&mut self, references: usize) -> Result<(), TryReserveError> {
        self.narrow.try_reserve(references)
    }

    /// Inserts like [`Self::insert`] but reserves the target map first, so a failed allocation is an error.
    pub fn try_insert(&mut self, reference: u128, id: u32) -> Result<Option<u32>, TryReserveError> {
        match u64::try_from(reference) {
            Ok(narrow) => {
                self.narrow.try_reserve(1)?;
                Ok(self.narrow.insert(narrow, id))
            }
            Err(_) => {
                self.wide.try_reserve(1)?;
                Ok(self.wide.insert(reference, id))
            }
        }
    }

    pub fn len(&self) -> usize {
        self.narrow.len() + self.wide.len()
    }

    pub fn is_empty(&self) -> bool {
        self.narrow.is_empty() && self.wide.is_empty()
    }

    pub fn get(&self, reference: &u128) -> Option<&u32> {
        match u64::try_from(*reference) {
            Ok(narrow) => self.narrow.get(&narrow),
            Err(_) => self.wide.get(reference),
        }
    }

    pub fn contains_key(&self, reference: &u128) -> bool {
        self.get(reference).is_some()
    }

    pub fn insert(&mut self, reference: u128, id: u32) -> Option<u32> {
        match u64::try_from(reference) {
            Ok(narrow) => self.narrow.insert(narrow, id),
            Err(_) => self.wide.insert(reference, id),
        }
    }

    pub fn remove(&mut self, reference: &u128) -> Option<u32> {
        match u64::try_from(*reference) {
            Ok(narrow) => self.narrow.remove(&narrow),
            Err(_) => self.wide.remove(reference),
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(u128, &u32) -> bool) {
        self.narrow.retain(|&key, id| keep(u128::from(key), id));
        self.wide.retain(|&key, id| keep(key, id));
    }

    /// Rewrites every id through `new_id` and drops references it rejects. Capacity is kept.
    pub fn remap(&mut self, mut new_id: impl FnMut(u32) -> Option<u32>) {
        self.narrow.retain(|_, id| remapped(&mut new_id, id));
        self.wide.retain(|_, id| remapped(&mut new_id, id));
    }

    pub fn clear(&mut self) {
        self.narrow.clear();
        self.wide.clear();
    }

    pub fn shrink_to_fit(&mut self) {
        self.narrow.shrink_to_fit();
        self.wide.shrink_to_fit();
    }

    pub fn iter(&self) -> impl Iterator<Item = (u128, u32)> + '_ {
        let narrow = self.narrow.iter().map(|(&key, &id)| (u128::from(key), id));
        narrow.chain(self.wide.iter().map(|(&key, &id)| (key, id)))
    }

    pub fn values(&self) -> impl Iterator<Item = &u32> + '_ {
        self.narrow.values().chain(self.wide.values())
    }

    /// Heap bytes of both maps: one bucket plus one control byte per allocated slot.
    pub fn memory_bytes(&self) -> usize {
        table_bytes::<(u64, u32)>(self.narrow.capacity())
            + table_bytes::<(u128, u32)>(self.wide.capacity())
    }
}

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
    fn memory_counts_allocated_buckets() {
        let mut map = FrnMap::with_capacity(100);
        map.try_insert(WIDE, 0).unwrap();
        // 100 usable entries need 128 buckets; 3 wide entries fit in 4.
        assert_eq!(map.memory_bytes(), 128 * 17 + 4 * 33);
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
    fn remap_rewrites_both_widths_through_a_lookup_and_keeps_capacity() {
        let mut map: FrnMap = [(5, 0), (WIDE, 3), (9, 1), (10, 2), (11, 7)].into();
        let (narrow, wide) = (map.narrow.capacity(), map.wide.capacity());
        let moved = [Some(0), None, Some(1), Some(2)];
        map.remap(|id| moved.get(id as usize).copied().flatten());
        assert_eq!(map, [(5, 0), (WIDE, 2), (10, 1)].into());
        assert_eq!((map.narrow.capacity(), map.wide.capacity()), (narrow, wide));
    }

    #[test]
    fn retain_sees_both_widths() {
        let mut map: FrnMap = [(5, 0), (WIDE, 1), (9, 2)].into();
        map.retain(|reference, &id| reference == WIDE || id == 2);
        assert_eq!(map, [(WIDE, 1), (9, 2)].into());
    }
}
