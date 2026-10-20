use std::collections::{HashMap, HashSet};

use rusty_core::ProviderPath;

use crate::table::{Flags, TableChange};

pub type FrnMap = HashMap<u128, u32>;

const FILE_CREATE: u32 = 0x100;
const FILE_DELETE: u32 = 0x200;
const RENAME_OLD_NAME: u32 = 0x1000;
const RENAME_NEW_NAME: u32 = 0x2000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsnRecord {
    pub file_id: u128,
    pub parent_id: u128,
    pub usn: i64,
    pub reason: u32,
    pub flags: Flags,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsnBatch {
    pub next_usn: i64,
    pub records: Vec<UsnRecord>,
}

// A new name follows its old name within a read or two; a longer wait means the pair is lost.
pub const MAX_CARRIED_READS: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingRenames {
    waiting: Vec<(u128, u32)>,
}

impl PendingRenames {
    pub fn clear(&mut self) {
        self.waiting.clear();
    }

    fn settle(&mut self, mut open: HashSet<u128>, seen: &[u128], aged: bool) -> Vec<TableChange> {
        let mut removed = Vec::new();
        for (file_id, reads) in std::mem::take(&mut self.waiting) {
            if !open.remove(&file_id) {
                continue;
            }
            let reads = reads + u32::from(aged);
            if reads > MAX_CARRIED_READS {
                removed.push(TableChange::Remove { file_id });
            } else {
                self.waiting.push((file_id, reads));
            }
        }
        for &file_id in seen {
            if open.remove(&file_id) {
                self.waiting.push((file_id, 0));
            }
        }
        removed
    }
}

impl UsnBatch {
    pub fn into_changes(self, renames: &mut PendingRenames) -> Vec<TableChange> {
        let mut changes = Vec::new();
        let mut pending: HashSet<u128> = renames.waiting.iter().map(|&(id, _)| id).collect();
        let mut old_ids = Vec::new();
        let aged = !self.records.is_empty();
        for record in self.records {
            if record.reason & RENAME_OLD_NAME != 0 {
                old_ids.push(record.file_id);
            }
            if let Some(change) = map_record(record, &mut pending) {
                changes.push(change);
            }
        }
        changes.extend(renames.settle(pending, &old_ids, aged));
        changes
    }
}

fn map_record(record: UsnRecord, pending: &mut HashSet<u128>) -> Option<TableChange> {
    let file_id = record.file_id;
    if record.reason & FILE_DELETE != 0 {
        pending.remove(&file_id);
        return Some(TableChange::Remove { file_id });
    }
    if record.reason & RENAME_NEW_NAME != 0 {
        let paired = pending.remove(&file_id);
        return Some(named_change(record, paired));
    }
    if record.reason & RENAME_OLD_NAME != 0 {
        pending.insert(file_id);
        return None;
    }
    if record.reason & FILE_CREATE != 0 {
        pending.remove(&file_id);
        return Some(named_change(record, false));
    }
    (record.reason != 0).then_some(TableChange::Attributes {
        file_id,
        flags: record.flags,
    })
}

fn named_change(record: UsnRecord, rename: bool) -> TableChange {
    let UsnRecord {
        file_id,
        parent_id,
        name,
        flags,
        ..
    } = record;
    if rename {
        TableChange::Rename {
            file_id,
            parent_id,
            name,
            flags,
        }
    } else {
        TableChange::Add {
            file_id,
            parent_id,
            name,
            flags,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalPosition {
    pub journal_id: u64,
    pub next_usn: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    Continue,
    Reenumerate,
}

impl JournalPosition {
    pub fn resume(self, journal_id: u64, lowest_valid_usn: i64) -> Resume {
        if self.journal_id != journal_id || self.next_usn < lowest_valid_usn {
            Resume::Reenumerate
        } else {
            Resume::Continue
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid USN batch at byte {offset}: {kind}")]
pub struct ParseError {
    pub offset: usize,
    pub kind: ParseErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseErrorKind {
    #[error("truncated header or record")]
    Truncated,
    #[error("invalid record length")]
    InvalidRecordLength,
    #[error("invalid name length or offset")]
    InvalidName,
    #[error("invalid range extent length")]
    InvalidExtents,
    #[error("unsupported record major version {0}")]
    UnsupportedVersion(u16),
}

pub fn parse_batch(bytes: &[u8]) -> Result<UsnBatch, ParseError> {
    let next_usn =
        i64::from_le_bytes(read(bytes, 0).map_err(|kind| ParseError { offset: 0, kind })?);
    let mut remaining = bytes.get(8..).ok_or(ParseError {
        offset: 0,
        kind: ParseErrorKind::Truncated,
    })?;
    let mut records = Vec::new();
    while !remaining.is_empty() {
        let offset = bytes.len() - remaining.len();
        let (length, record) =
            parse_record(remaining).map_err(|kind| ParseError { offset, kind })?;
        if let Some(record) = record {
            records.push(record);
        }
        remaining = remaining.get(length..).ok_or(ParseError {
            offset,
            kind: ParseErrorKind::Truncated,
        })?;
    }
    Ok(UsnBatch { next_usn, records })
}

fn read<const N: usize>(bytes: &[u8], at: usize) -> Result<[u8; N], ParseErrorKind> {
    bytes
        .get(at..)
        .and_then(|tail| tail.get(..N))
        .and_then(|value| value.try_into().ok())
        .ok_or(ParseErrorKind::Truncated)
}

fn parse_record(bytes: &[u8]) -> Result<(usize, Option<UsnRecord>), ParseErrorKind> {
    let length = u32::from_le_bytes(read(bytes, 0)?) as usize;
    if length < 8 || !length.is_multiple_of(8) {
        return Err(ParseErrorKind::InvalidRecordLength);
    }
    let record = bytes.get(..length).ok_or(ParseErrorKind::Truncated)?;
    let version = u16::from_le_bytes(read(record, 4)?);
    let parsed = match version {
        2 | 3 => Some(parse_named_record(record, version)?),
        4 => {
            validate_ranges(record)?;
            None
        }
        other => return Err(ParseErrorKind::UnsupportedVersion(other)),
    };
    Ok((length, parsed))
}

fn parse_named_record(bytes: &[u8], version: u16) -> Result<UsnRecord, ParseErrorKind> {
    let width = if version == 2 { 8 } else { 16 };
    let fields = 40 + width * 2;
    let length = usize::from(u16::from_le_bytes(read(bytes, fields)?));
    let offset = usize::from(u16::from_le_bytes(read(bytes, fields + 2)?));
    if !length.is_multiple_of(2) || !offset.is_multiple_of(2) || offset < fields + 4 {
        return Err(ParseErrorKind::InvalidName);
    }
    let end = offset
        .checked_add(length)
        .ok_or(ParseErrorKind::InvalidName)?;
    let name_bytes = bytes.get(offset..end).ok_or(ParseErrorKind::InvalidName)?;
    let (units, _) = name_bytes.as_chunks::<2>();
    let units: Vec<_> = units.iter().copied().map(u16::from_le_bytes).collect();
    let reason_at = 24 + width * 2;
    Ok(UsnRecord {
        file_id: read_file_id(bytes, 8, width)?,
        parent_id: read_file_id(bytes, 8 + width, width)?,
        usn: i64::from_le_bytes(read(bytes, 8 + width * 2)?),
        reason: u32::from_le_bytes(read(bytes, reason_at)?),
        flags: attribute_flags(u32::from_le_bytes(read(bytes, reason_at + 12)?)),
        name: ProviderPath::from_wide(&units).to_text(),
    })
}

fn read_file_id(bytes: &[u8], at: usize, width: usize) -> Result<u128, ParseErrorKind> {
    if width == 8 {
        Ok(u128::from(u64::from_le_bytes(read(bytes, at)?)))
    } else {
        Ok(u128::from_le_bytes(read(bytes, at)?))
    }
}

fn attribute_flags(attributes: u32) -> Flags {
    let mut flags = Flags::NONE;
    for (mask, flag) in [
        (0x10, Flags::FOLDER),
        (0x2, Flags::HIDDEN),
        (0x4, Flags::SYSTEM),
        (0x400, Flags::REPARSE),
    ] {
        if attributes & mask != 0 {
            flags = flags | flag;
        }
    }
    flags
}

fn validate_ranges(bytes: &[u8]) -> Result<(), ParseErrorKind> {
    let count = usize::from(u16::from_le_bytes(read(bytes, 60)?));
    let size = usize::from(u16::from_le_bytes(read(bytes, 62)?));
    let end = count
        .checked_mul(size)
        .and_then(|length| length.checked_add(64))
        .ok_or(ParseErrorKind::InvalidExtents)?;
    if size < 16 || bytes.get(..end).is_none() {
        return Err(ParseErrorKind::InvalidExtents);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
