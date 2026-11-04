mod detect;
mod provider;
mod sevenz;
mod source;
mod stream;
mod tar_headers;
mod tarball;
mod zipped;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub use detect::detect;
pub use provider::{
    ArchiveFs, ArchiveProblem, READ_ONLY, UnsafeReason, location, provider_id, raw_location,
};

pub const ENTRY_LIMIT: u64 = 64 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 200_000;
pub(crate) const NOT_ARCHIVE: &str = "This is not an archive Navo can read.";
const UNPACK_BUDGET: u64 = 2 * 1024 * 1024 * 1024;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchiveKind {
    #[default]
    File,
    Folder,
    Symlink,
    Hardlink,
    CharDevice,
    BlockDevice,
    Fifo,
}

impl ArchiveKind {
    #[must_use]
    pub fn is_folder(self) -> bool {
        self == Self::Folder
    }

    #[must_use]
    pub fn is_special(self) -> bool {
        matches!(
            self,
            Self::Symlink | Self::Hardlink | Self::CharDevice | Self::BlockDevice | Self::Fifo
        )
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

pub(crate) fn unix_ms(stamp: Option<Stamp>) -> Option<i64> {
    match stamp? {
        Stamp::UnixMs(millis) => Some(millis),
        Stamp::Local(_) => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub packed: Option<u64>,
    pub modified: Option<Stamp>,
    pub kind: ArchiveKind,
    pub is_encrypted: bool,
    pub header_offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveListing {
    pub format: ArchiveFormat,
    pub entries: Vec<ArchiveEntry>,
    pub truncated: bool,
}

pub const ENCRYPTED: &str = "Encrypted: not supported yet.";
pub const UNSUPPORTED_METHOD: &str = "Compression method not supported yet.";
pub const SYMLINK: &str = "This is a symbolic link inside an archive; it cannot be copied out.";
pub const HARDLINK: &str = "This is a hard link inside an archive; it cannot be copied out.";
pub const CHAR_DEVICE: &str =
    "This is a character device inside an archive; it cannot be copied out.";
pub const BLOCK_DEVICE: &str = "This is a block device inside an archive; it cannot be copied out.";
pub const FIFO: &str = "This is a named pipe inside an archive; it cannot be copied out.";
pub(crate) const OVER_BUDGET: &str =
    "Over 2 GB to unpack before this entry: too much to view from the archive.";
pub(crate) const CANCELLED: &str = "Stopped.";

#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits<'a> {
    pub entries: usize,
    pub budget: u64,
    pub cancel: &'a AtomicBool,
}

impl<'a> Limits<'a> {
    fn new(cancel: &'a AtomicBool) -> Self {
        Self {
            entries: MAX_ENTRIES,
            budget: UNPACK_BUDGET,
            cancel,
        }
    }
}

pub(crate) struct Listed<'a> {
    entries: Vec<ArchiveEntry>,
    truncated: bool,
    limits: Limits<'a>,
}

impl<'a> Listed<'a> {
    pub(crate) fn new(limits: Limits<'a>) -> Self {
        Self {
            entries: Vec::new(),
            truncated: false,
            limits,
        }
    }

    pub(crate) fn push(&mut self, entry: ArchiveEntry) -> bool {
        let limits = self.limits;
        if self.entries.len() >= limits.entries || limits.cancel.load(Ordering::Relaxed) {
            self.truncated = true;
            return false;
        }
        self.entries.push(entry);
        true
    }

    pub(crate) fn cut(mut self) -> Self {
        self.truncated = true;
        self
    }
}

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
    let format = detect(&head, &name).ok_or_else(|| NOT_ARCHIVE.to_owned())?;
    let file = File::open(path).map_err(|error| format!("Cannot open it: {error}"))?;
    Ok((file, format))
}

pub(crate) type ZipReader = zip::ZipArchive<BufReader<File>>;

pub(crate) fn list_with(path: &Path, limits: Limits<'_>) -> Result<ArchiveListing, String> {
    list_with_handle(path, limits).map(|(listing, _)| listing)
}

pub(crate) fn list_with_handle(
    path: &Path,
    limits: Limits<'_>,
) -> Result<(ArchiveListing, Option<ZipReader>), String> {
    let (file, format) = open(path)?;
    let reader = BufReader::new(file);
    let (listed, zip) = match format {
        ArchiveFormat::Zip => {
            let mut archive = zipped::open(reader)?;
            let listed = zipped::list_archive(&mut archive, limits)?;
            (listed, Some(archive))
        }
        ArchiveFormat::SevenZip => (sevenz::list(reader, limits)?, None),
        _ => (tarball::list(format, reader, limits)?, None),
    };
    Ok((
        ArchiveListing {
            format,
            entries: listed.entries,
            truncated: listed.truncated,
        },
        zip,
    ))
}

pub fn list_archive(path: &Path, cancel: &AtomicBool) -> Result<ArchiveListing, String> {
    list_with(path, Limits::new(cancel))
}

pub(crate) fn read_with(path: &Path, at: EntryAt, limits: Limits<'_>) -> Result<Vec<u8>, String> {
    let (file, format) = open(path)?;
    let reader = BufReader::new(file);
    match format {
        ArchiveFormat::Zip => zipped::read(reader, at),
        ArchiveFormat::SevenZip => sevenz::read(reader, at, limits),
        _ => tarball::read(format, reader, at, limits),
    }
}

pub fn read_archive_entry(
    path: &Path,
    at: EntryAt,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    read_with(path, at, Limits::new(cancel))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryAt {
    pub index: usize,
    pub limit: u64,
}

impl EntryAt {
    pub(crate) fn check(self, entry: &ArchiveEntry) -> Result<(), String> {
        if entry.kind.is_folder() {
            return Err("This is a folder.".to_owned());
        }
        if let Some(reason) = special_refused(entry.kind) {
            return Err(reason.to_owned());
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

pub(crate) fn special_refused(kind: ArchiveKind) -> Option<&'static str> {
    match kind {
        ArchiveKind::Symlink => Some(SYMLINK),
        ArchiveKind::Hardlink => Some(HARDLINK),
        ArchiveKind::CharDevice => Some(CHAR_DEVICE),
        ArchiveKind::BlockDevice => Some(BLOCK_DEVICE),
        ArchiveKind::Fifo => Some(FIFO),
        ArchiveKind::File | ArchiveKind::Folder => None,
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
