use std::collections::{HashMap, VecDeque};
use std::io;

#[cfg(any(windows, test))]
mod record_size;
mod snapshot;
#[cfg(any(windows, test, feature = "test-support"))]
#[cfg_attr(
    all(not(windows), not(test)),
    allow(
        dead_code,
        reason = "test-support uses enumeration without Windows journal I/O"
    )
)]
mod volume_io;
#[cfg(any(windows, test, feature = "test-support"))]
use volume_io::*;

use crate::table::{Flags, NameTable, TableChange, TableError};
use crate::usn::{FrnMap, JournalPosition, ParseError, PendingRenames, UsnBatch};
#[cfg(any(windows, test, feature = "test-support"))]
use crate::{
    table::NO_PARENT,
    usn::{Resume, parse_batch},
};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::Volume;

const ROOT_REFERENCE: u128 = 5;

#[derive(Debug, thiserror::Error)]
pub enum NtfsError {
    #[error("NTFS {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("a volume must be an ASCII drive letter")]
    InvalidVolume,
    #[error("invalid NTFS response: {0}")]
    InvalidResponse(&'static str),
    #[error(transparent)]
    Snapshot(#[from] crate::file::FileError),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Table(#[from] TableError),
    #[error("the journal changed or wrapped; enumerate the volume again")]
    Reenumerate,
    #[error("the file reference was deleted or reused")]
    FileGone,
    #[error("NTFS enumeration cancelled")]
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct NtfsIndex {
    pub table: NameTable,
    pub ids: FrnMap,
    pub position: JournalPosition,
    renames: PendingRenames,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnresolvedChange {
    pub missing_reference: u128,
    pub change: TableChange,
}

#[derive(Debug)]
pub struct Enumeration {
    pub index: NtfsIndex,
    pub unresolved: Vec<UnresolvedChange>,
    pub sizes_unknown: u64,
}

impl NtfsIndex {
    #[cfg(any(windows, test, feature = "test-support"))]
    pub(crate) fn new(drive: char, position: JournalPosition) -> Result<Self, NtfsError> {
        volume_path(drive)?;
        let mut table = NameTable::new();
        let root = table.push(
            NO_PARENT,
            &format!("{}:", drive.to_ascii_uppercase()),
            Flags::FOLDER,
        )?;
        Ok(Self {
            table,
            ids: [(ROOT_REFERENCE, root)].into(),
            position,
            renames: PendingRenames::default(),
        })
    }

    pub fn pending_renames(&self) -> &PendingRenames {
        &self.renames
    }

    pub fn restart_at(&mut self, position: JournalPosition) {
        self.position = position;
        self.renames.clear();
    }

    pub fn apply_batch(&mut self, batch: UsnBatch) -> Result<Vec<UnresolvedChange>, NtfsError> {
        let mut applying = self.begin_batch(batch)?;
        while applying.step(self, usize::MAX)? {}
        applying.finish(self)
    }

    pub fn begin_batch(&self, batch: UsnBatch) -> Result<BatchApply, NtfsError> {
        BatchApply::new(batch, self.position, self.renames.clone())
    }
}

pub struct BatchApply {
    position: JournalPosition,
    next_usn: i64,
    renames: Option<PendingRenames>,
    changes: std::iter::Peekable<std::vec::IntoIter<TableChange>>,
    pending: Pending,
}

impl BatchApply {
    pub fn new(
        batch: UsnBatch,
        position: JournalPosition,
        mut renames: PendingRenames,
    ) -> Result<Self, NtfsError> {
        validate_journal_cursor(&batch, position)?;
        Ok(Self {
            position,
            next_usn: batch.next_usn,
            changes: batch.into_changes(&mut renames).into_iter().peekable(),
            renames: Some(renames),
            pending: Pending::default(),
        })
    }

    pub fn step(&mut self, index: &mut NtfsIndex, limit: usize) -> Result<bool, NtfsError> {
        if index.position != self.position {
            return Err(NtfsError::InvalidResponse(
                "the index moved during a journal batch",
            ));
        }
        if let Some(renames) = self.renames.take() {
            index.renames = renames;
        }
        let mut applied = 0;
        while let Some(change) = self.changes.next() {
            let file_id = change_file_id(&change);
            let pushed = self.pending.push(index, change);
            if pushed.is_err() {
                self.pending.forget_removed(index);
                pushed?;
            }
            applied += 1;
            let size_follows = matches!(
                self.changes.peek(),
                Some(TableChange::Size { file_id: next, .. }) if *next == file_id
            );
            if applied >= limit && !size_follows {
                break;
            }
        }
        self.pending.forget_removed(index);
        Ok(self.changes.peek().is_some())
    }

    pub fn finish(mut self, index: &mut NtfsIndex) -> Result<Vec<UnresolvedChange>, NtfsError> {
        while self.step(index, usize::MAX)? {}
        index.position.next_usn = self.next_usn;
        Ok(self.pending.finish())
    }
}

fn validate_journal_cursor(batch: &UsnBatch, saved: JournalPosition) -> Result<(), NtfsError> {
    if batch.next_usn < saved.next_usn
        || (batch.next_usn == saved.next_usn && !batch.records.is_empty())
    {
        return Err(NtfsError::InvalidResponse("journal cursor did not advance"));
    }
    Ok(())
}

struct Blocked {
    missing: u128,
    changes: VecDeque<TableChange>,
}

#[derive(Default)]
struct Pending {
    blocked: HashMap<u128, Blocked>,
    waiters: HashMap<u128, Vec<u128>>,
    reported: Vec<UnresolvedChange>,
    removed_subtree: bool,
}

impl Pending {
    fn push(&mut self, index: &mut NtfsIndex, change: TableChange) -> Result<(), NtfsError> {
        let Some(change) = normalize_root(index, change) else {
            return Ok(());
        };
        let file_id = change_file_id(&change);
        if let Some(blocked) = self.blocked.get_mut(&file_id) {
            blocked.changes.push_back(change);
            return Ok(());
        }
        let mut ready = VecDeque::from([VecDeque::from([change])]);
        while let Some(changes) = ready.pop_front() {
            self.apply_queue(index, changes, &mut ready)?;
        }
        Ok(())
    }

    fn apply_queue(
        &mut self,
        index: &mut NtfsIndex,
        mut changes: VecDeque<TableChange>,
        ready: &mut VecDeque<VecDeque<TableChange>>,
    ) -> Result<(), NtfsError> {
        while let Some(change) = changes.pop_front() {
            let change = recover_rename(index, change);
            let file_id = change_file_id(&change);
            match index.table.apply_in_batch(change.clone(), &mut index.ids) {
                Ok(removed) => {
                    self.removed_subtree |= removed > 1;
                    self.wake(index, file_id, ready);
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
                    self.blocked.insert(file_id, Blocked { missing, changes });
                    self.waiters.entry(missing).or_default().push(file_id);
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn wake(
        &mut self,
        index: &NtfsIndex,
        file_id: u128,
        ready: &mut VecDeque<VecDeque<TableChange>>,
    ) {
        if !index.ids.contains_key(&file_id) {
            return;
        }
        for waiting in self.waiters.remove(&file_id).into_iter().flatten() {
            if let Some(blocked) = self.blocked.remove(&waiting) {
                ready.push_back(blocked.changes);
            }
        }
    }

    fn forget_removed(&mut self, index: &mut NtfsIndex) {
        if std::mem::take(&mut self.removed_subtree) {
            index.table.forget_removed(&mut index.ids);
        }
    }

    fn finish(self) -> Vec<UnresolvedChange> {
        let unresolved = self.blocked.into_values().flat_map(|blocked| {
            blocked
                .changes
                .into_iter()
                .map(move |change| UnresolvedChange {
                    missing_reference: blocked.missing,
                    change,
                })
        });
        self.reported.into_iter().chain(unresolved).collect()
    }
}

fn change_file_id(change: &TableChange) -> u128 {
    match change {
        TableChange::Add { file_id, .. }
        | TableChange::Rename { file_id, .. }
        | TableChange::Size { file_id, .. }
        | TableChange::Remove { file_id }
        | TableChange::Attributes { file_id, .. } => *file_id,
        TableChange::JournalReset => ROOT_REFERENCE,
    }
}

fn is_root(reference: u128) -> bool {
    reference <= u128::from(u64::MAX) && reference & 0x0000_ffff_ffff_ffff == ROOT_REFERENCE
}

fn normalize_root(index: &mut NtfsIndex, mut change: TableChange) -> Option<TableChange> {
    let file_id = change_file_id(&change);
    if is_root(file_id) {
        if let Some(&root) = index.ids.get(&ROOT_REFERENCE) {
            index.ids.insert(file_id, root);
        }
        return match change {
            TableChange::Add { flags, .. }
            | TableChange::Rename { flags, .. }
            | TableChange::Attributes { flags, .. } => Some(TableChange::Attributes {
                file_id: ROOT_REFERENCE,
                flags: flags | Flags::FOLDER,
            }),
            _ => None,
        };
    }
    if let TableChange::Add { parent_id, .. } | TableChange::Rename { parent_id, .. } = &mut change
        && is_root(*parent_id)
    {
        *parent_id = ROOT_REFERENCE;
    }
    Some(change)
}

fn recover_rename(index: &NtfsIndex, change: TableChange) -> TableChange {
    match change {
        TableChange::Rename {
            file_id,
            parent_id,
            name,
            flags,
        } if index
            .ids
            .get(&file_id)
            .is_none_or(|&id| index.table.name(id).is_none()) =>
        {
            TableChange::Add {
                file_id,
                parent_id,
                name,
                flags,
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod batch_tests;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(test)]
mod tests;
