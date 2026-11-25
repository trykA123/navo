use super::NtfsIndex;

/// Below this many slots a sparse table costs less than an O(n) rebuild is worth.
pub const COMPACT_MIN_SLOTS: u32 = 100_000;

impl NtfsIndex {
    /// Renumbers the table and every file reference once dead slots pass a third of all slots.
    /// Returns whether it renumbered. Otherwise reclaims old names when the arena warrants it.
    /// Each reclamation costs O(n) under the index write lock.
    pub(super) fn compact_if_sparse(&mut self, min_slots: u32) -> bool {
        if !self.table.is_sparse(min_slots) {
            self.table.reclaim_names();
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
