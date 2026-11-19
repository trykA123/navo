//! Resolves journal hard-link changes to the file name selected by MFT enumeration.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::{NtfsError, UsnBatch, VolumeIo};
use crate::usn::{UsnRecord, parse_first};

const HARD_LINK_CHANGE: u32 = 0x10000;
const FILE_CREATE: u32 = 0x100;
const FILE_DELETE: u32 = 0x200;

/// Rewrites hard-link records as upserts or removals after all MFT lookups succeed.
pub(super) fn refresh_links(
    volume: &mut impl VolumeIo,
    batch: &mut UsnBatch,
) -> Result<(), NtfsError> {
    let mut refreshed = HashMap::new();
    for record in &batch.records {
        if record.reason & HARD_LINK_CHANGE != 0
            && let Entry::Vacant(entry) = refreshed.entry(record.file_id)
        {
            entry.insert(lookup_link(volume, record.file_id)?);
        }
    }
    if refreshed.is_empty() {
        return Ok(());
    }
    for record in &mut batch.records {
        if record.reason & HARD_LINK_CHANGE == 0 {
            continue;
        }
        match &refreshed[&record.file_id] {
            Some(current) => {
                record.name.clone_from(&current.name);
                record.parent_id = current.parent_id;
                record.flags = current.flags;
                record.reason = FILE_CREATE;
            }
            None => record.reason = FILE_DELETE,
        }
    }
    Ok(())
}

/// Reads the first MFT record at the requested slot and checks the full file reference.
/// Fails with `LinkRefresh` (carrying the read error) or `Reenumerate("hard link")`.
fn lookup_link(volume: &mut impl VolumeIo, file_id: u128) -> Result<Option<UsnRecord>, NtfsError> {
    let reference = u64::try_from(file_id).map_err(|_| NtfsError::Reenumerate("hard link"))?;
    let cursor = reference & 0x0000_ffff_ffff_ffff;
    let Some(bytes) = volume
        .enumerate_one(cursor)
        .map_err(|error| NtfsError::LinkRefresh(Box::new(error)))?
    else {
        return Ok(None);
    };
    let record = parse_first(&bytes).map_err(|_| NtfsError::Reenumerate("hard link"))?;
    Ok(record.filter(|record| record.file_id == file_id))
}
