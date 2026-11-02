use std::io::{self, Read};

use crate::model::{
    ContentClass, ContentKey, EntryKind, HashCache, ObjectId, Reason, SideEntry, SideSource, Stamp,
    Status,
};

use super::{ContentResult, Side};

pub(super) type Readers = (Box<dyn Read + Send>, Box<dyn Read + Send>);

pub(super) fn read_pair(
    left: &mut dyn Read,
    right: &mut dyn Read,
    left_buffer: &mut [u8],
    right_buffer: &mut [u8],
) -> Result<(usize, usize), ContentResult> {
    let left_read = read_block(left, left_buffer).map_err(|_| unavailable(Reason::ReadError))?;
    let right_read = read_block(right, right_buffer).map_err(|_| unavailable(Reason::ReadError))?;
    Ok((left_read, right_read))
}

pub(super) fn open_pair(
    left: Side<'_>,
    right: Side<'_>,
    hint: crate::model::ReadHint,
) -> Result<Readers, ContentResult> {
    let left_reader = match left.source.open(&left.entry.rel, hint) {
        Ok(reader) => reader,
        Err(reason) => return Err(unavailable(reason)),
    };
    match right.source.open(&right.entry.rel, hint) {
        Ok(right_reader) => Ok((left_reader, right_reader)),
        Err(reason) => Err(unavailable(reason)),
    }
}

pub(super) fn check_kinds(left: &SideEntry, right: &SideEntry) -> Option<ContentResult> {
    if left.kind != right.kind {
        return Some(outcome(Status::TypeConflict, ContentClass::Unknown));
    }
    match left.kind {
        EntryKind::File => None,
        EntryKind::Folder => Some(outcome(Status::Same, ContentClass::Unknown)),
        EntryKind::Link => Some(unavailable(Reason::Link)),
        EntryKind::Submodule => Some(unavailable(Reason::Submodule)),
    }
}

pub(super) fn check_object_ids(left: Side<'_>, right: Side<'_>) -> Option<ContentResult> {
    let left_id = left.source.object_id(&left.entry.rel);
    let right_id = right.source.object_id(&right.entry.rel);
    match (left_id, right_id) {
        (Some(left_id), Some(right_id)) if left_id == right_id => {
            Some(outcome(Status::Same, ContentClass::Unknown))
        }
        (Some(ObjectId::Sha1(_)), Some(ObjectId::Sha1(_)))
        | (Some(ObjectId::Sha256(_)), Some(ObjectId::Sha256(_))) => {
            Some(different(None, ContentClass::Unknown))
        }
        _ => None,
    }
}

pub(super) fn content_key(source: &dyn SideSource, entry: &SideEntry) -> ContentKey {
    ContentKey {
        provider: source.provider().clone(),
        full_path: source.cache_path(&entry.rel),
        size: entry.size,
        modified: entry.modified,
        file_id: source.file_id(&entry.rel),
    }
}

pub(super) fn read_block(reader: &mut dyn Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

pub(super) fn first_difference(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right)
        .position(|(left, right)| left != right)
        .unwrap_or(left.len().min(right.len()))
}

pub(super) fn stamp_matches(stamp: &Stamp, entry: &SideEntry) -> bool {
    stamp.size == entry.size && stamp.modified == entry.modified
}

pub(super) fn finish_side(
    cache: &dyn HashCache,
    key: &ContentKey,
    hashes: ([u8; 32], [u8; 32]),
    saw_nul: bool,
) -> ContentResult {
    if hashes.0 == hashes.1 {
        cache.put(key, hashes.0);
        return outcome(Status::Same, class(saw_nul));
    }
    different(None, different_class(saw_nul))
}

pub(super) fn unavailable(reason: Reason) -> ContentResult {
    outcome(Status::Unavailable(reason), ContentClass::Unknown)
}

pub(super) fn changed() -> ContentResult {
    unavailable(Reason::ChangedOnDisk)
}

pub(super) fn stopped() -> ContentResult {
    outcome(Status::NotChecked(Reason::Stopped), ContentClass::Unknown)
}

pub(super) const fn class(saw_nul: bool) -> ContentClass {
    if saw_nul {
        ContentClass::Binary
    } else {
        ContentClass::Text
    }
}

pub(super) const fn different_class(saw_nul: bool) -> ContentClass {
    if saw_nul {
        ContentClass::Binary
    } else {
        ContentClass::Unknown
    }
}

pub(super) fn different(first_difference: Option<u64>, content: ContentClass) -> ContentResult {
    outcome(Status::Different { first_difference }, content)
}

pub(super) const fn outcome(status: Status, content: ContentClass) -> ContentResult {
    ContentResult { status, content }
}
