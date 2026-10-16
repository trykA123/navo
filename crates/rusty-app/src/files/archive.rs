use std::sync::atomic::AtomicBool;

use rusty_core::Location;
use rusty_fs::archive::{ArchiveListing, ENTRY_LIMIT, EntryAt, list_archive, read_archive_entry};

use super::path_of;

pub fn archive(location: &Location, cancel: &AtomicBool) -> Result<ArchiveListing, String> {
    list_archive(&path_of(location)?, cancel)
}

pub fn archive_entry(
    location: &Location,
    index: usize,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let at = EntryAt {
        index,
        limit: ENTRY_LIMIT,
    };
    read_archive_entry(&path_of(location)?, at, cancel)
}
