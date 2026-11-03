use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::PathKind;

mod report;

pub(crate) use report::DiskExport;

const FILE_LIMIT: u64 = 200_000;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskKind {
    ListingCache,
    Journal,
    Session,
    Index,
    LogsAndCrashes,
    PerformanceRecordings,
}

impl DiskKind {
    fn label(self) -> &'static str {
        match self {
            Self::ListingCache => "Listing cache",
            Self::Journal => "Journal",
            Self::Session => "Session",
            Self::Index => "Index",
            Self::LogsAndCrashes => "Logs and crash reports",
            Self::PerformanceRecordings => "Performance recordings",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiskLocation {
    kind: DiskKind,
    path: PathBuf,
    side_files: Vec<PathBuf>,
}

impl DiskLocation {
    #[must_use]
    pub fn new(kind: DiskKind, path: PathBuf) -> Self {
        Self {
            kind,
            path,
            side_files: Vec::new(),
        }
    }

    #[must_use]
    pub fn sqlite(kind: DiskKind, path: PathBuf) -> Self {
        let side_files = ["-wal", "-shm"]
            .into_iter()
            .map(|suffix| side_path(&path, suffix))
            .collect();
        Self {
            kind,
            path,
            side_files,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn side_path(path: &Path, suffix: &str) -> PathBuf {
    let mut file = path.as_os_str().to_owned();
    file.push(suffix);
    PathBuf::from(file)
}

#[derive(Debug, Clone, Default)]
pub struct DiskUse {
    pub locations: Vec<DiskLocationUse>,
    pub total: DiskTotals,
}

#[derive(Debug, Clone)]
pub struct DiskLocationUse {
    pub kind: DiskKind,
    pub path: PathBuf,
    pub parts: Vec<DiskPart>,
    pub usage: DiskTotals,
    path_kind: PathKind,
}

#[derive(Debug, Clone)]
pub struct DiskPart {
    pub path: PathBuf,
    pub usage: DiskTotals,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskState {
    #[default]
    Complete,
    AtLeast,
    NotReadable,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DiskTotals {
    pub files: u64,
    pub bytes: u64,
    pub state: DiskState,
}

impl DiskTotals {
    fn add(&mut self, other: &Self) {
        self.files = self.files.saturating_add(other.files);
        self.bytes = self.bytes.saturating_add(other.bytes);
    }

    fn mark_partial(&mut self) {
        if self.state == DiskState::Complete {
            self.state = DiskState::AtLeast;
        }
    }

    fn mark_unreadable(&mut self) {
        self.state = DiskState::NotReadable;
    }

    fn is_empty(&self) -> bool {
        self.files == 0 && self.bytes == 0 && self.state == DiskState::Complete
    }
}

pub fn scan_disk_use(locations: &[DiskLocation]) -> DiskUse {
    scan_with_limit(locations, FILE_LIMIT)
}

fn scan_with_limit(locations: &[DiskLocation], limit: u64) -> DiskUse {
    scan_with_reader(locations, limit, &FsReader)
}

fn scan_with_reader(locations: &[DiskLocation], limit: u64, reader: &dyn DiskReader) -> DiskUse {
    let mut disk = DiskUse::default();
    for location in locations {
        let usage = measure(&location.path, location.kind, limit, reader);
        let parts: Vec<DiskPart> = location
            .side_files
            .iter()
            .filter_map(|path| {
                let usage = measure(path, location.kind, limit, reader);
                (!usage.is_empty()).then(|| DiskPart {
                    path: path.clone(),
                    usage,
                })
            })
            .collect();
        let usage = grouped(usage, &parts);
        disk.total.files = disk.total.files.saturating_add(usage.files);
        disk.total.bytes = disk.total.bytes.saturating_add(usage.bytes);
        if usage.state != DiskState::Complete {
            disk.total.mark_partial();
        }
        disk.locations.push(DiskLocationUse {
            kind: location.kind,
            path: location.path.clone(),
            parts,
            usage,
            path_kind: path_kind(reader, &location.path),
        });
    }
    disk
}

fn grouped(mut usage: DiskTotals, parts: &[DiskPart]) -> DiskTotals {
    let state = if usage.state == DiskState::NotReadable {
        DiskState::NotReadable
    } else if usage.state == DiskState::AtLeast
        || parts
            .iter()
            .any(|part| part.usage.state != DiskState::Complete)
    {
        DiskState::AtLeast
    } else {
        DiskState::Complete
    };
    for part in parts {
        usage.add(&part.usage);
    }
    usage.state = state;
    usage
}

fn path_kind(reader: &dyn DiskReader, path: &Path) -> PathKind {
    match reader.metadata(path) {
        Ok(info) if info.kind == EntryKind::File => PathKind::File,
        _ => PathKind::Folder,
    }
}

trait DiskReader {
    fn metadata(&self, path: &Path) -> io::Result<EntryInfo>;
    fn folder(&self, path: &Path) -> io::Result<Listing>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Folder,
    Link,
    Other,
}

#[derive(Debug, Clone, Copy)]
struct EntryInfo {
    kind: EntryKind,
    len: u64,
}

#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    kind: Option<EntryKind>,
}

#[derive(Debug, Clone)]
struct Listing {
    entries: Vec<Entry>,
    complete: bool,
}

struct FsReader;

impl DiskReader for FsReader {
    fn metadata(&self, path: &Path) -> io::Result<EntryInfo> {
        let metadata = fs::symlink_metadata(path)?;
        Ok(EntryInfo {
            kind: metadata_kind(&metadata),
            len: metadata.len(),
        })
    }

    fn folder(&self, path: &Path) -> io::Result<Listing> {
        let mut listing = Listing {
            entries: Vec::new(),
            complete: true,
        };
        for entry in fs::read_dir(path)? {
            match entry {
                Ok(entry) => listing.entries.push(Entry {
                    path: entry.path(),
                    kind: entry.file_type().ok().map(file_type_kind),
                }),
                Err(_) => listing.complete = false,
            }
        }
        Ok(listing)
    }
}

fn metadata_kind(metadata: &fs::Metadata) -> EntryKind {
    if is_link(metadata) {
        EntryKind::Link
    } else if metadata.is_file() {
        EntryKind::File
    } else if metadata.is_dir() {
        EntryKind::Folder
    } else {
        EntryKind::Other
    }
}

fn file_type_kind(kind: fs::FileType) -> EntryKind {
    if kind.is_symlink() {
        EntryKind::Link
    } else if kind.is_file() {
        EntryKind::File
    } else if kind.is_dir() {
        EntryKind::Folder
    } else {
        EntryKind::Other
    }
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

struct Scan {
    usage: DiskTotals,
    pending: Vec<PathBuf>,
    visited: u64,
    root: PathBuf,
}

fn measure(path: &Path, kind: DiskKind, limit: u64, reader: &dyn DiskReader) -> DiskTotals {
    let mut scan = Scan {
        usage: DiskTotals::default(),
        pending: Vec::new(),
        visited: 0,
        root: path.to_path_buf(),
    };
    if path.ancestors().any(|ancestor| {
        reader
            .metadata(ancestor)
            .is_ok_and(|info| info.kind == EntryKind::Link)
    }) {
        return scan.usage;
    }
    scan.pending.push(path.to_path_buf());
    while let Some(current) = scan.pending.pop() {
        let info = match reader.metadata(&current) {
            Ok(info) => info,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) if denied(&error, &current, &scan) => {
                scan.usage.mark_unreadable();
                return scan.usage;
            }
            Err(_) => {
                scan.usage.mark_partial();
                continue;
            }
        };
        match info.kind {
            EntryKind::Link => continue,
            EntryKind::File => {
                scan.usage.files = scan.usage.files.saturating_add(1);
                scan.usage.bytes = scan.usage.bytes.saturating_add(info.len);
                if scan.usage.files >= limit {
                    scan.usage.mark_partial();
                    return scan.usage;
                }
            }
            EntryKind::Folder => collect_entries(&current, kind, &mut scan, reader),
            EntryKind::Other => {}
        }
    }
    scan.usage
}

fn denied(error: &io::Error, path: &Path, scan: &Scan) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied && path == scan.root
}

fn collect_entries(path: &Path, kind: DiskKind, scan: &mut Scan, reader: &dyn DiskReader) {
    if scan.visited >= FILE_LIMIT {
        scan.usage.mark_partial();
        return;
    }
    let listing = match reader.folder(path) {
        Ok(listing) => listing,
        Err(error) if denied(&error, path, scan) => {
            scan.usage.mark_unreadable();
            return;
        }
        Err(_) => {
            scan.usage.mark_partial();
            return;
        }
    };
    if !listing.complete {
        scan.usage.mark_partial();
    }
    for entry in listing.entries {
        scan.visited = scan.visited.saturating_add(1);
        let keep = if matches!(kind, DiskKind::PerformanceRecordings) {
            entry.kind == Some(EntryKind::File) && is_recording(&entry.path)
        } else {
            true
        };
        if keep {
            scan.pending.push(entry.path);
        }
        if scan.visited >= FILE_LIMIT {
            scan.usage.mark_partial();
            break;
        }
    }
}

fn is_recording(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy())
        .is_some_and(|name| name.starts_with("navo-performance-") && name.ends_with(".zip"))
}

#[cfg(test)]
mod tests;
