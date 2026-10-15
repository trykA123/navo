use rusty_core::Location;
use rusty_fs::archive::{ArchiveListing, ENTRY_LIMIT, list_archive, read_archive_entry};

use super::path_of;

pub fn archive(location: &Location) -> Result<ArchiveListing, String> {
    list_archive(&path_of(location)?)
}

pub fn archive_entry(location: &Location, index: usize) -> Result<Vec<u8>, String> {
    read_archive_entry(&path_of(location)?, index, ENTRY_LIMIT)
}
