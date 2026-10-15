mod detect;
mod sevenz;
mod tarball;
mod zipped;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

pub use detect::detect;

pub const ENTRY_LIMIT: u64 = 64 * 1024 * 1024;
const HEAD: u64 = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarZst,
    SevenZip,
}

impl ArchiveFormat {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "Zip",
            Self::Tar => "Tar",
            Self::TarGz => "Tar · gzip",
            Self::TarXz => "Tar · xz",
            Self::TarZst => "Tar · zstd",
            Self::SevenZip => "7z",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stamp {
    UnixMs(i64),
    Local(Civil),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub packed: Option<u64>,
    pub modified: Option<Stamp>,
    pub is_folder: bool,
    pub is_encrypted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveListing {
    pub format: ArchiveFormat,
    pub entries: Vec<ArchiveEntry>,
}

pub const ENCRYPTED: &str = "Encrypted: not supported yet.";

pub(crate) fn damaged(error: impl std::fmt::Display) -> String {
    format!("This archive looks damaged: {error}")
}

fn open(path: &Path) -> Result<(File, ArchiveFormat), String> {
    let mut file = File::open(path).map_err(|error| format!("Cannot open it: {error}"))?;
    let mut head = Vec::new();
    file.by_ref()
        .take(HEAD)
        .read_to_end(&mut head)
        .map_err(|error| format!("Cannot open it: {error}"))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let format = detect(&head, &name)
        .ok_or_else(|| "This is not an archive Ferrite can read.".to_owned())?;
    let file = File::open(path).map_err(|error| format!("Cannot open it: {error}"))?;
    Ok((file, format))
}

pub fn list_archive(path: &Path) -> Result<ArchiveListing, String> {
    let (file, format) = open(path)?;
    let entries = match format {
        ArchiveFormat::Zip => zipped::list(BufReader::new(file))?,
        ArchiveFormat::SevenZip => sevenz::list(BufReader::new(file))?,
        _ => tarball::list(format, BufReader::new(file))?,
    };
    Ok(ArchiveListing { format, entries })
}

pub fn read_archive_entry(path: &Path, index: usize, limit: u64) -> Result<Vec<u8>, String> {
    let (file, format) = open(path)?;
    let at = Entry { index, limit };
    match format {
        ArchiveFormat::Zip => zipped::read(BufReader::new(file), at),
        ArchiveFormat::SevenZip => sevenz::read(BufReader::new(file), at),
        _ => tarball::read(format, BufReader::new(file), at),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry {
    pub index: usize,
    pub limit: u64,
}

impl Entry {
    pub(crate) fn check(self, entry: &ArchiveEntry) -> Result<(), String> {
        if entry.is_folder {
            return Err("This is a folder.".to_owned());
        }
        if entry.is_encrypted {
            return Err(ENCRYPTED.to_owned());
        }
        if entry.size > self.limit {
            return Err(too_large(self.limit));
        }
        Ok(())
    }
}

fn too_large(limit: u64) -> String {
    format!(
        "Over {} MB: too large to view from the archive.",
        limit / (1024 * 1024)
    )
}

pub(crate) fn read_capped(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(damaged)?;
    if bytes.len() as u64 > limit {
        return Err(too_large(limit));
    }
    Ok(bytes)
}

pub(crate) fn missing() -> String {
    "This entry is no longer in the archive.".to_owned()
}

#[cfg(test)]
mod tests;
