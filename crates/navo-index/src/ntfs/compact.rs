use super::NtfsIndex;

/// Below this many slots a sparse table costs less than an O(n) rebuild is worth.
pub const COMPACT_MIN_SLOTS: u32 = 100_000;

impl NtfsIndex {
    /// Renumbers the table and every file reference once dead slots outnumber live ones.
    /// Returns whether it compacted; callers must hold no table ids across this call.
    /// Costs O(n) under the index write lock, once per doubling of dead slots.
    pub(super) fn compact_if_sparse(&mut self, min_slots: u32) -> bool {
        let slots = self.table.slots();
        if slots < min_slots || slots as usize <= 2 * self.table.len() {
            return false;
        }
        let map = self.table.compact();
        self.ids.remap(&map);
        true
    }
}

#[cfg(test)]
#[path = "compact_tests.rs"]
mod tests;
