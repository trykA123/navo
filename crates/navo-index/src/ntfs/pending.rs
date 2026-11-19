//! Applies changes in dependency order while retaining compact records for unseen parents.

use std::collections::{HashMap, VecDeque};

use super::{
    NtfsError, NtfsIndex, UnresolvedChange, change_file_id, normalize_root, recover_rename,
};
use crate::table::{Flags, TableChange, TableError};
use crate::usn::FrnMap;

#[derive(Default)]
pub(super) struct Pending {
    blocked: BlockedRecords,
    deferred_deletes: Vec<DeferredDelete>,
    reported: Vec<UnresolvedChange>,
    removed_subtree: bool,
    pub(super) blocked_peak: usize,
    pub(super) cycle_skips: u64,
}

struct DeferredDelete {
    file_id: u128,
    blocker: u128,
    changes: VecDeque<TableChange>,
}

impl Pending {
    /// Applies a change or waits for its missing parent, preserving changes for each file in order.
    pub(super) fn push(
        &mut self,
        index: &mut NtfsIndex,
        change: TableChange,
    ) -> Result<(), NtfsError> {
        let Some(change) = normalize_root(index, change) else {
            return Ok(());
        };
        let file_id = change_file_id(&change);
        if let Some(delete) = self
            .deferred_deletes
            .iter_mut()
            .find(|delete| delete.file_id == file_id)
        {
            delete.changes.push_back(change);
            return Ok(());
        }
        if matches!(change, TableChange::Remove { .. })
            && let Some(blocker) = self.blocked.blocking_descendant(index, file_id)
        {
            self.deferred_deletes.push(DeferredDelete {
                file_id,
                blocker,
                changes: VecDeque::from([change]),
            });
            return Ok(());
        }
        self.apply_one(index, change)?;
        while let Some(change) = self.take_deferred(index) {
            self.apply_one(index, change)?;
        }
        Ok(())
    }

    /// Applies a change and drains the waiter groups it wakes.
    fn apply_one(&mut self, index: &mut NtfsIndex, change: TableChange) -> Result<(), NtfsError> {
        let change = if let TableChange::Remove { file_id } = change {
            self.blocked.cancel(file_id);
            change
        } else if let Some(change) = self.blocked.append(change_file_id(&change), change) {
            change
        } else {
            self.note_blocked();
            return Ok(());
        };
        let mut ready = VecDeque::new();
        self.apply_queue(index, VecDeque::from([change]), &mut ready)?;
        while let Some((_, changes)) = self.blocked.pop_ready(&mut ready) {
            self.apply_queue(index, changes, &mut ready)?;
        }
        Ok(())
    }

    /// Takes the next safe change once all ready moves have run.
    fn take_deferred(&mut self, index: &NtfsIndex) -> Option<TableChange> {
        for i in 0..self.deferred_deletes.len() {
            let delete = &mut self.deferred_deletes[i];
            if matches!(delete.changes.front(), Some(TableChange::Remove { .. }))
                && let Some(blocker) = self.blocked.blocking_descendant(index, delete.file_id)
            {
                delete.blocker = blocker;
                continue;
            }
            let change = delete.changes.pop_front();
            if delete.changes.is_empty() {
                self.deferred_deletes.remove(i);
            }
            return change;
        }
        None
    }

    /// Applies one file's ordered changes; only Add and Rename can wait for a parent.
    fn apply_queue(
        &mut self,
        index: &mut NtfsIndex,
        mut changes: VecDeque<TableChange>,
        ready: &mut VecDeque<Ready>,
    ) -> Result<(), NtfsError> {
        while let Some(change) = changes.pop_front() {
            let change = recover_rename(index, change);
            let file_id = change_file_id(&change);
            match index.table.apply_in_batch(change.clone(), &mut index.ids) {
                Ok(removed) => {
                    self.removed_subtree |= removed > 1;
                    self.wake_if_present(index, file_id, ready);
                }
                Err(TableError::UnknownFileReference(missing))
                    if matches!(
                        change,
                        TableChange::Attributes { .. } | TableChange::Size { .. }
                    ) =>
                {
                    self.reported.push(UnresolvedChange {
                        missing_reference: missing,
                        change,
                    });
                }
                Err(TableError::UnknownFileReference(missing)) => {
                    changes.push_front(change);
                    self.blocked.insert(missing, changes)?;
                    self.note_blocked();
                    return Ok(());
                }
                Err(TableError::Cycle(_)) => self.cycle_skips += 1,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// Wakes the files waiting for `file_id` as their parent, once that file exists in the index.
    fn wake_if_present(&mut self, index: &NtfsIndex, file_id: u128, ready: &mut VecDeque<Ready>) {
        if index.ids.contains_key(&file_id) {
            self.blocked.wake(file_id, ready);
        }
    }

    /// Counts blocked files, including neither later changes nor files already in the ready queue.
    fn note_blocked(&mut self) {
        self.blocked_peak = self.blocked_peak.max(self.blocked.len());
    }

    /// Removes stale file references once a batch has deleted an entire subtree.
    pub(super) fn forget_removed(&mut self, index: &mut NtfsIndex) {
        if std::mem::take(&mut self.removed_subtree) {
            index.table.forget_removed(&mut index.ids);
        }
    }

    /// Returns unresolved changes, keeping their missing references and per-file order.
    pub(super) fn finish(mut self) -> Vec<UnresolvedChange> {
        self.reported.extend(self.blocked.finish());
        for delete in self.deferred_deletes {
            self.reported
                .extend(delete.changes.into_iter().map(|change| UnresolvedChange {
                    missing_reference: delete.blocker,
                    change,
                }));
        }
        self.reported
    }
}

const RENAME: u8 = 0x10;
const SIZED: u8 = 0x20;
const CANCELLED: u8 = 0x40;

/// Flat columns use 37 bytes per slot; the missing parent lives once in the waiter index.
#[derive(Default)]
struct BlockedRecords {
    files: Vec<u128>,
    ranges: Vec<(u32, u32)>,
    sizes: Vec<u64>,
    metadata: Vec<u8>,
    links: Vec<u32>,
    names: String,
    blocked: FrnMap,
    waiters: FrnMap,
    later: HashMap<u32, VecDeque<TableChange>>,
    free: Option<u32>,
    live: usize,
}

/// A circular waiter list keeps siblings together in the breadth-first ready queue.
struct Ready {
    missing: u128,
    head: u32,
    tail: u32,
}

impl BlockedRecords {
    fn len(&self) -> usize {
        self.blocked.len()
    }

    /// Finds a blocked move whose indexed entry would be removed with the folder.
    fn blocking_descendant(&self, index: &NtfsIndex, file_id: u128) -> Option<u128> {
        let folder = *index.ids.get(&file_id)?;
        if self.blocked.is_empty() || !index.table.flags(folder)?.contains(Flags::FOLDER) {
            return None;
        }
        self.blocked.iter().find_map(|(file, _)| {
            let id = *index.ids.get(&file)?;
            std::iter::successors(index.table.parent(id), |&id| index.table.parent(id))
                .any(|parent| parent == folder)
                .then_some(file)
        })
    }

    /// Discards a file's blocked changes and leaves its slot linked until the waiter is taken.
    fn cancel(&mut self, file: u128) {
        if let Some(slot) = self.blocked.remove(&file) {
            self.metadata[slot as usize] |= CANCELLED;
            self.later.remove(&slot);
        }
    }

    /// Appends to an already blocked file, folding its first Size into the flat record.
    fn append(&mut self, file: u128, change: TableChange) -> Option<TableChange> {
        let Some(&slot) = self.blocked.get(&file) else {
            return Some(change);
        };
        self.append_to_slot(slot, change);
        None
    }

    /// Folds only the first Size; subsequent journal changes keep their original sequence.
    fn append_to_slot(&mut self, slot: u32, change: TableChange) {
        if let TableChange::Size { bytes, .. } = change
            && self.metadata[slot as usize] & SIZED == 0
            && !self.later.contains_key(&slot)
        {
            self.sizes[slot as usize] = bytes;
            self.metadata[slot as usize] |= SIZED;
        } else {
            self.later.entry(slot).or_default().push_back(change);
        }
    }

    /// Stores the first named change and indexes its file and missing parent; overflow returns Full.
    fn insert(
        &mut self,
        missing: u128,
        mut changes: VecDeque<TableChange>,
    ) -> Result<(), NtfsError> {
        let first = changes
            .pop_front()
            .ok_or(NtfsError::InvalidResponse("empty deferred queue"))?;
        let (file, name, flags, kind) = match first {
            TableChange::Add {
                file_id,
                name,
                flags,
                ..
            } => (file_id, name, flags, 0),
            TableChange::Rename {
                file_id,
                name,
                flags,
                ..
            } => (file_id, name, flags, RENAME),
            _ => return Err(NtfsError::InvalidResponse("deferred change has no parent")),
        };
        let start = u32::try_from(self.names.len()).map_err(|_| TableError::Full)?;
        let end = self
            .names
            .len()
            .checked_add(name.len())
            .ok_or(TableError::Full)?;
        let end = u32::try_from(end).map_err(|_| TableError::Full)?;
        reserve_names(&mut self.names, name.len());
        self.names.push_str(&name);
        let slot = self.insert_columns(file, (start, end), flags.bits() as u8 | kind)?;
        self.blocked.insert(file, slot);
        if let Some(tail) = self.waiters.insert(missing, slot) {
            self.links[slot as usize] = self.links[tail as usize];
            self.links[tail as usize] = slot;
        }
        for change in changes {
            self.append_to_slot(slot, change);
        }
        Ok(())
    }

    /// Allocates or reuses one column slot, linking a new waiter to itself.
    fn insert_columns(
        &mut self,
        file: u128,
        range: (u32, u32),
        metadata: u8,
    ) -> Result<u32, TableError> {
        if let Some(slot) = self.free {
            self.live += 1;
            self.free =
                (self.links[slot as usize] != u32::MAX).then_some(self.links[slot as usize]);
            self.files[slot as usize] = file;
            self.ranges[slot as usize] = range;
            self.sizes[slot as usize] = 0;
            self.metadata[slot as usize] = metadata;
            self.links[slot as usize] = slot;
            return Ok(slot);
        }
        let slot = u32::try_from(self.files.len()).map_err(|_| TableError::Full)?;
        if slot == u32::MAX {
            return Err(TableError::Full);
        }
        self.live += 1;
        push_column(&mut self.files, file);
        push_column(&mut self.ranges, range);
        push_column(&mut self.sizes, 0);
        push_column(&mut self.metadata, metadata);
        push_column(&mut self.links, slot);
        Ok(slot)
    }

    /// Removes all siblings from the blocked count before any of them runs, preserving blocked_peak.
    fn wake(&mut self, missing: u128, ready: &mut VecDeque<Ready>) {
        let Some(tail) = self.waiters.remove(&missing) else {
            return;
        };
        let head = self.links[tail as usize];
        let mut slot = head;
        loop {
            // A cancelled file may be blocked again in a newer slot.
            if self.metadata[slot as usize] & CANCELLED == 0 {
                self.blocked.remove(&self.files[slot as usize]);
            }
            if slot == tail {
                break;
            }
            slot = self.links[slot as usize];
        }
        ready.push_back(Ready {
            missing,
            head,
            tail,
        });
    }

    /// Materializes only the next file's changes, leaving its siblings ahead of newly ready children.
    fn pop_ready(&mut self, ready: &mut VecDeque<Ready>) -> Option<(u128, VecDeque<TableChange>)> {
        let mut group = ready.pop_front()?;
        let slot = group.head;
        let missing = group.missing;
        if slot != group.tail {
            group.head = self.links[slot as usize];
            ready.push_front(group);
        }
        Some((missing, self.take(slot, missing)))
    }

    /// Returns the slot's changes, or an empty queue if cancelled, and releases the slot.
    fn take(&mut self, slot: u32, parent_id: u128) -> VecDeque<TableChange> {
        let i = slot as usize;
        let changes = if self.metadata[i] & CANCELLED == 0 {
            self.restore_changes(slot, parent_id)
        } else {
            VecDeque::new()
        };
        self.links[i] = self.free.unwrap_or(u32::MAX);
        self.free = Some(slot);
        self.live -= 1;
        if self.live == 0 {
            // Release the arena before the completed table trims its capacity.
            *self = Self::default();
        }
        changes
    }

    /// Reconstructs a live slot's changes in their original order.
    fn restore_changes(&mut self, slot: u32, parent_id: u128) -> VecDeque<TableChange> {
        let i = slot as usize;
        let (start, end) = self.ranges[i];
        let file_id = self.files[i];
        let name = self.names[start as usize..end as usize].to_owned();
        let flags = restore_flags(self.metadata[i]);
        let change = if self.metadata[i] & RENAME == 0 {
            TableChange::Add {
                file_id,
                parent_id,
                name,
                flags,
            }
        } else {
            TableChange::Rename {
                file_id,
                parent_id,
                name,
                flags,
            }
        };
        let mut changes = VecDeque::from([change]);
        if self.metadata[i] & SIZED != 0 {
            changes.push_back(TableChange::Size {
                file_id,
                bytes: self.sizes[i],
            });
        }
        changes.extend(self.later.remove(&slot).into_iter().flatten());
        changes
    }

    /// Returns every unresolved change with its missing parent and original per-file order.
    fn finish(mut self) -> Vec<UnresolvedChange> {
        let mut ready = self
            .waiters
            .iter()
            .map(|(missing, tail)| Ready {
                missing,
                head: self.links[tail as usize],
                tail,
            })
            .collect();
        let mut unresolved = Vec::new();
        while let Some((missing_reference, changes)) = self.pop_ready(&mut ready) {
            unresolved.extend(changes.into_iter().map(|change| UnresolvedChange {
                missing_reference,
                change,
            }));
        }
        unresolved
    }
}

/// Restores the four table attribute bits without exposing the queue's metadata bits.
fn restore_flags(metadata: u8) -> Flags {
    [Flags::FOLDER, Flags::HIDDEN, Flags::SYSTEM, Flags::REPARSE]
        .into_iter()
        .filter(|flag| u16::from(metadata) & flag.bits() != 0)
        .fold(Flags::NONE, |flags, flag| flags | flag)
}

/// Grows a flat column by a sixteenth to bound spare bytes while the finished table grows.
fn push_column<T>(column: &mut Vec<T>, value: T) {
    if column.len() == column.capacity() {
        column.reserve_exact((column.len() / 16).max(1_024));
    }
    column.push(value);
}

/// Reserves name bytes by a sixteenth, including the current name, with no per-record allocation.
fn reserve_names(names: &mut String, needed: usize) {
    if names.capacity() - names.len() < needed {
        names.reserve_exact((names.len() / 16).max(16 * 1_024).max(needed));
    }
}
