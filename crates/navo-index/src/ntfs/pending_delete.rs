//! Bounds the work needed to check a recursive journal removal for blocked moves.

use std::collections::VecDeque;

use super::{BlockedRecords, CANCELLED, NtfsIndex, Ready};

const DELETE_SCAN_LIMIT: usize = 4_096;
const DELETE_SCAN_STEPS: usize = 16 * DELETE_SCAN_LIMIT;

/// Returns the table entry if removing the file reference would follow child links.
pub(super) fn indexed_subtree(index: &NtfsIndex, file_id: u128) -> Option<u32> {
    index
        .ids
        .get(&file_id)
        .copied()
        .filter(|&id| index.table.has_children(id))
}

impl BlockedRecords {
    /// Returns an unavailable waiter parent if deleting the subtree may lose a blocked move.
    pub(super) fn blocking_parent(
        &self,
        index: &NtfsIndex,
        folder: u32,
        ready: &VecDeque<Ready>,
    ) -> Option<u128> {
        let any_parent = || self.waiters.iter().next().map(|(missing, _)| missing);
        if self.len() > DELETE_SCAN_LIMIT {
            return any_parent();
        }
        let mut steps = DELETE_SCAN_STEPS;
        match self.blocking_slot(index, folder, &mut steps) {
            Ok(Some(slot)) => return self.waiter_parent(slot, steps).or_else(any_parent),
            Ok(None) => {}
            Err(()) => return any_parent(),
        }
        // Woken moves are absent from the blocked map until the ready queue applies them.
        ready.iter().find_map(|group| {
            let mut slots = std::iter::successors(Some(group.head), |&slot| {
                (slot != group.tail).then(|| self.links[slot as usize])
            });
            let unsafe_delete = slots.any(|slot| {
                steps = steps.saturating_sub(1);
                let i = slot as usize;
                steps == 0
                    || self.metadata[i] & CANCELLED == 0
                        && has_ancestor(index, self.files[i], folder, &mut steps).unwrap_or(true)
            });
            unsafe_delete.then_some(group.missing)
        })
    }

    /// Finds a blocked descendant's slot, or returns an error when the work budget runs out.
    fn blocking_slot(
        &self,
        index: &NtfsIndex,
        folder: u32,
        steps: &mut usize,
    ) -> Result<Option<u32>, ()> {
        if self.blocked.is_empty() {
            return Ok(None);
        }
        // The flat slots avoid scanning empty pages and buckets in the sparse reference map.
        for (slot, file) in self.files.iter().enumerate() {
            *steps = steps.checked_sub(1).ok_or(())?;
            let Some(&blocked) = self.blocked.get(file).filter(|&&id| id as usize == slot) else {
                continue;
            };
            if has_ancestor(index, *file, folder, steps)? {
                return Ok(Some(blocked));
            }
        }
        Ok(None)
    }

    /// Looks up a slot's missing parent within the remaining budget for maps and ring links.
    fn waiter_parent(&self, target: u32, steps: usize) -> Option<u128> {
        // Charge the map's footprint conservatively before its iterator probes sparse storage.
        let mut remaining = steps
            .checked_sub(self.waiters.memory_bytes())?
            .min(DELETE_SCAN_LIMIT);
        if remaining == 0 {
            return None;
        }
        self.waiters.iter().find_map(|(missing, tail)| {
            let mut ring = std::iter::successors(Some(tail), |&slot| {
                let next = self.links[slot as usize];
                (next != tail).then_some(next)
            });
            let found_or_exhausted = ring.any(|slot| {
                remaining = remaining.saturating_sub(1);
                slot == target || remaining == 0
            });
            found_or_exhausted.then_some(missing)
        })
    }
}

/// Walks an indexed entry's parents and returns an error when the work budget runs out.
fn has_ancestor(
    index: &NtfsIndex,
    file: u128,
    ancestor: u32,
    steps: &mut usize,
) -> Result<bool, ()> {
    let Some(mut id) = index.ids.get(&file).copied() else {
        return Ok(false);
    };
    loop {
        *steps = steps.checked_sub(1).ok_or(())?;
        let Some(parent) = index.table.parent(id) else {
            return Ok(false);
        };
        if parent == ancestor {
            return Ok(true);
        }
        id = parent;
    }
}
