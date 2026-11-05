use super::NtfsIndex;

/// Below this many slots a sparse table costs less than an O(n) rebuild is worth.
pub const COMPACT_MIN_SLOTS: u32 = 100_000;

impl NtfsIndex {
    /// Renumbers the table and every file reference once dead slots pass a third of all slots.
    /// Returns whether it compacted; callers must hold no table ids across this call.
    /// Costs O(n) under the index write lock, once dead slots reach half the live count.
    pub(super) fn compact_if_sparse(&mut self, min_slots: u32) -> bool {
        if !self.table.is_sparse(min_slots) {
            return false;
        }
        let map = self.table.compact();
        self.ids.remap(|id| map.get(id));
        drop(map);
        self.ids.shrink_to_fit();
        true
    }
}

#[cfg(test)]
#[path = "compact_tests.rs"]
mod tests;
