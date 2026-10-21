use std::collections::{HashMap, VecDeque};
use std::io;

mod snapshot;

use crate::table::{Flags, NO_PARENT, NameTable, TableChange, TableError};
use crate::usn::{
    FrnMap, JournalPosition, ParseError, PendingRenames, Resume, UsnBatch, parse_batch,
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
}

#[derive(Debug)]
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
}

impl NtfsIndex {
    fn new(drive: char, position: JournalPosition) -> Result<Self, NtfsError> {
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

    pub fn restart_at(&mut self, position: JournalPosition) {
        self.position = position;
        self.renames.clear();
    }

    pub fn apply_batch(&mut self, batch: UsnBatch) -> Result<Vec<UnresolvedChange>, NtfsError> {
        validate_journal_cursor(&batch, self.position)?;
        let next_usn = batch.next_usn;
        let mut pending = Pending::default();
        for change in batch.into_changes(&mut self.renames) {
            pending.push(self, change)?;
        }
        self.position.next_usn = next_usn;
        Ok(pending.finish())
    }
}

trait VolumeIo {
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError>;
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError>;
    fn read_journal(&mut self, position: JournalPosition) -> Result<Vec<u8>, NtfsError>;
}

struct JournalState {
    position: JournalPosition,
    readable_from: i64,
}

impl JournalState {
    fn parse(bytes: &[u8]) -> Result<Self, NtfsError> {
        if bytes.len() < 56 {
            return Err(NtfsError::InvalidResponse("truncated journal metadata"));
        }
        let journal_id = u64::from_le_bytes(field(bytes, 0)?);
        let first = i64::from_le_bytes(field(bytes, 8)?);
        let next_usn = i64::from_le_bytes(field(bytes, 16)?);
        let lowest = i64::from_le_bytes(field(bytes, 24)?);
        if first < 0 || lowest < 0 || next_usn < first.max(lowest) {
            return Err(NtfsError::InvalidResponse("invalid journal bounds"));
        }
        Ok(Self {
            position: JournalPosition {
                journal_id,
                next_usn,
            },
            readable_from: first.max(lowest),
        })
    }

    fn check(&self, saved: JournalPosition) -> Result<(), NtfsError> {
        if saved.resume(self.position.journal_id, self.readable_from) == Resume::Reenumerate
            || saved.next_usn > self.position.next_usn
        {
            return Err(NtfsError::Reenumerate);
        }
        Ok(())
    }
}

fn enumerate_volume(
    volume: &mut impl VolumeIo,
    drive: char,
    limit: usize,
) -> Result<Enumeration, NtfsError> {
    let state = JournalState::parse(&volume.query_journal()?)?;
    let mut index = NtfsIndex::new(drive, state.position)?;
    let mut pending = Pending::default();
    let mut cursor = 0;
    let mut count = 0;
    while count < limit {
        let Some(bytes) = volume.enumerate(cursor)? else {
            break;
        };
        let batch = parse_batch(&bytes)?;
        let next = batch.next_usn.cast_unsigned();
        if next <= cursor {
            return Err(NtfsError::InvalidResponse(
                "enumeration cursor did not advance",
            ));
        }
        cursor = next;
        for record in batch.records.into_iter().take(limit - count) {
            count += 1;
            pending.push(
                &mut index,
                TableChange::Add {
                    file_id: record.file_id,
                    parent_id: record.parent_id,
                    name: record.name,
                    flags: record.flags,
                },
            )?;
        }
    }
    JournalState::parse(&volume.query_journal()?)?.check(state.position)?;
    Ok(Enumeration {
        index,
        unresolved: pending.finish(),
    })
}

fn read_volume_journal(
    volume: &mut impl VolumeIo,
    saved: JournalPosition,
) -> Result<UsnBatch, NtfsError> {
    JournalState::parse(&volume.query_journal()?)?.check(saved)?;
    let bytes = volume.read_journal(saved).map_err(journal_error)?;
    let batch = parse_batch(&bytes)?;
    validate_journal_cursor(&batch, saved)?;
    Ok(batch)
}

fn journal_error(error: NtfsError) -> NtfsError {
    const JOURNAL_ENTRY_DELETED: i32 = 1181;
    match error {
        NtfsError::Io { source, .. } if source.raw_os_error() == Some(JOURNAL_ENTRY_DELETED) => {
            NtfsError::Reenumerate
        }
        other => other,
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

fn field<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], NtfsError> {
    bytes
        .get(offset..)
        .and_then(|tail| tail.get(..N))
        .and_then(|value| value.try_into().ok())
        .ok_or(NtfsError::InvalidResponse("truncated metadata field"))
}

fn volume_path(drive: char) -> Result<Vec<u16>, NtfsError> {
    if !drive.is_ascii_alphabetic() {
        return Err(NtfsError::InvalidVolume);
    }
    Ok(format!("\\\\.\\{}:\0", drive.to_ascii_uppercase())
        .encode_utf16()
        .collect())
}

fn enum_request(cursor: u64) -> Vec<u8> {
    let mut bytes = vec![0; 24];
    bytes[..8].copy_from_slice(&cursor.to_le_bytes());
    bytes[16..24].copy_from_slice(&i64::MAX.to_le_bytes());
    bytes
}

fn journal_request(position: JournalPosition) -> Vec<u8> {
    let mut bytes = vec![0; 40];
    bytes[..8].copy_from_slice(&position.next_usn.to_le_bytes());
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    bytes[32..40].copy_from_slice(&position.journal_id.to_le_bytes());
    bytes
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
            match index.table.apply(change.clone(), &mut index.ids) {
                Ok(()) => self.wake(index, file_id, ready),
                Err(TableError::UnknownFileReference(missing))
                    if matches!(change, TableChange::Attributes { .. }) =>
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
mod tests;
