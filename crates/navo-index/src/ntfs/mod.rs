//! NTFS enumeration and journal updates over the shared index table.

#[cfg(test)]
use std::collections::HashMap;
use std::io;

mod compact;
mod pending;
use pending::Pending;
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
pub use compact::COMPACT_MIN_SLOTS;
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
    pub blocked_peak: usize,
}

/// Total capacities a build should reserve. `Default` reserves nothing; first builds use it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SizeHint {
    pub entries: usize,
    pub folders: usize,
    pub name_bytes: usize,
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
        index.compact_if_sparse(COMPACT_MIN_SLOTS);
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
#[cfg(test)]
mod blocked_peak_tests;
#[cfg(test)]
mod memory_tests;
#[cfg(test)]
mod peak_ratio_tests;
#[cfg(test)]
mod peak_tests;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(test)]
mod tests;
