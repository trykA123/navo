use std::fmt::Write;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{DriveKind, PathKind, Redactor};

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

#[derive(Debug, Clone, Default, Serialize)]
pub struct DiskUse {
    pub locations: Vec<DiskLocationUse>,
    pub total: DiskTotals,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiskLocationUse {
    pub kind: DiskKind,
    pub path: String,
    #[serde(flatten)]
    pub usage: DiskTotals,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DiskTotals {
    pub files: u64,
    pub bytes: u64,
    pub at_least: bool,
}

pub fn scan_disk_use(locations: &[(DiskKind, PathBuf)], redactor: &Redactor) -> DiskUse {
    scan_with_limit(locations, redactor, FILE_LIMIT)
}

fn scan_with_limit(locations: &[(DiskKind, PathBuf)], redactor: &Redactor, limit: u64) -> DiskUse {
    let mut disk = DiskUse::default();
    for (kind, path) in locations {
        let usage = measure(path, *kind, limit);
        disk.total.files = disk.total.files.saturating_add(usage.files);
        disk.total.bytes = disk.total.bytes.saturating_add(usage.bytes);
        disk.total.at_least |= usage.at_least;
        let path_kind = if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()) {
            PathKind::File
        } else {
            PathKind::Folder
        };
        disk.locations.push(DiskLocationUse {
            kind: *kind,
            path: redactor
                .path(&path.to_string_lossy(), path_kind, DriveKind::Unknown)
                .to_string(),
            usage,
        });
    }
    disk
}

#[derive(Default)]
struct Scan {
    usage: DiskTotals,
    pending: Vec<PathBuf>,
    visited: u64,
}

fn measure(path: &Path, kind: DiskKind, limit: u64) -> DiskTotals {
    let mut scan = Scan::default();
    if path
        .ancestors()
        .any(|ancestor| fs::symlink_metadata(ancestor).is_ok_and(|metadata| is_link(&metadata)))
    {
        return scan.usage;
    }
    scan.pending.push(path.to_path_buf());
    while let Some(path) = scan.pending.pop() {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                scan.usage.at_least = true;
                continue;
            }
        };
        if is_link(&metadata) {
            continue;
        }
        if metadata.is_file() {
            scan.usage.files += 1;
            scan.usage.bytes = scan.usage.bytes.saturating_add(metadata.len());
            if scan.usage.files >= limit {
                scan.usage.at_least = true;
                break;
            }
        } else if metadata.is_dir() {
            collect_entries(&path, kind, &mut scan);
        }
    }
    scan.usage
}

fn collect_entries(path: &Path, kind: DiskKind, scan: &mut Scan) {
    if scan.visited >= FILE_LIMIT {
        scan.usage.at_least = true;
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        scan.usage.at_least = true;
        return;
    };
    for entry in entries {
        scan.visited += 1;
        match entry {
            Ok(entry) if matches!(kind, DiskKind::PerformanceRecordings) => {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("navo-performance-")
                    && name.ends_with(".zip")
                    && entry.file_type().is_ok_and(|kind| kind.is_file())
                {
                    scan.pending.push(entry.path());
                }
            }
            Ok(entry) => scan.pending.push(entry.path()),
            Err(_) => scan.usage.at_least = true,
        }
        if scan.visited >= FILE_LIMIT {
            scan.usage.at_least = true;
            break;
        }
    }
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

impl DiskUse {
    pub(crate) fn summary(&self) -> String {
        let mut text = String::from("\nDisk use:\n");
        for location in &self.locations {
            let _ = writeln!(
                text,
                "  {}: {}: {}{} bytes, {} files",
                location.kind.label(),
                location.path,
                lower_bound(&location.usage),
                location.usage.bytes,
                location.usage.files,
            );
        }
        let _ = writeln!(
            text,
            "Total: {}{} bytes, {} files",
            lower_bound(&self.total),
            self.total.bytes,
            self.total.files,
        );
        text
    }
}

fn lower_bound(usage: &DiskTotals) -> &'static str {
    if usage.at_least { "at least " } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Identity, Salt};

    fn redactor() -> Redactor {
        Redactor::new(Salt::from_bytes([7; 16]), Identity::default())
    }

    #[test]
    fn sums_known_files_in_each_location_and_total() {
        let profile = tempfile::tempdir().unwrap();
        let data = profile.path().join("private-profile");
        fs::create_dir_all(data.join("index/nested")).unwrap();
        fs::write(data.join("cache.db"), [0; 7]).unwrap();
        fs::write(data.join("index/one"), [0; 11]).unwrap();
        fs::write(data.join("index/nested/two"), [0; 13]).unwrap();
        let disk = scan_disk_use(
            &[
                (DiskKind::ListingCache, data.join("cache.db")),
                (DiskKind::Index, data.join("index")),
            ],
            &redactor(),
        );
        assert_eq!((disk.total.files, disk.total.bytes), (3, 31));
        assert_eq!(disk.locations[1].usage.bytes, 24);
        assert!(!disk.total.at_least);
        assert!(
            !serde_json::to_string(&disk)
                .unwrap()
                .contains("private-profile")
        );
    }

    #[test]
    fn file_cap_marks_the_location_and_total_as_lower_bounds() {
        let root = tempfile::tempdir().unwrap();
        for name in ["one", "two", "three"] {
            fs::write(root.path().join(name), [0; 5]).unwrap();
        }
        let disk = scan_with_limit(&[(DiskKind::Index, root.path().to_owned())], &redactor(), 2);
        assert_eq!((disk.total.files, disk.total.bytes), (2, 10));
        assert!(disk.locations[0].usage.at_least && disk.total.at_least);
    }

    #[cfg(unix)]
    #[test]
    fn skips_symlinked_files_folders_and_location_ancestors() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), [0; 19]).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), root.path().join("file"))
            .unwrap();
        for path in [root.path().to_owned(), root.path().join("link/secret")] {
            let disk = scan_disk_use(&[(DiskKind::Index, path)], &redactor());
            assert_eq!((disk.total.files, disk.total.bytes), (0, 0));
        }
    }

    #[test]
    fn recordings_include_only_navo_performance_zips() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("navo-performance-test.zip"), [0; 9]).unwrap();
        fs::write(root.path().join("personal.zip"), [0; 19]).unwrap();
        fs::create_dir(root.path().join("folder")).unwrap();
        fs::write(
            root.path().join("folder/navo-performance-test.zip"),
            [0; 29],
        )
        .unwrap();
        let disk = scan_disk_use(
            &[(DiskKind::PerformanceRecordings, root.path().to_owned())],
            &redactor(),
        );
        assert_eq!((disk.total.files, disk.total.bytes), (1, 9));
    }
}
