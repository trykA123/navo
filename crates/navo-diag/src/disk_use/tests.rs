use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::*;

fn fake_folder(path: &str) -> PathBuf {
    PathBuf::from(path)
}

#[derive(Default)]
struct FakeReader {
    folders: HashMap<PathBuf, Vec<(PathBuf, EntryKind)>>,
    denied: HashSet<PathBuf>,
}

impl DiskReader for FakeReader {
    fn metadata(&self, path: &Path) -> io::Result<EntryInfo> {
        if self.folders.contains_key(path) {
            return Ok(EntryInfo {
                kind: EntryKind::Folder,
                len: 0,
            });
        }
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn folder(&self, path: &Path) -> io::Result<Listing> {
        if self.denied.contains(path) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        let entries = self
            .folders
            .get(path)
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        Ok(Listing {
            entries: entries
                .iter()
                .map(|(path, kind)| Entry {
                    path: path.clone(),
                    kind: Some(*kind),
                })
                .collect(),
            complete: true,
        })
    }
}

#[test]
fn sums_known_files_in_each_location_and_total() {
    let profile = tempfile::tempdir().unwrap();
    let data = profile.path().join("private-profile");
    fs::create_dir_all(data.join("index/nested")).unwrap();
    fs::write(data.join("cache.db"), [0; 7]).unwrap();
    fs::write(data.join("index/one"), [0; 11]).unwrap();
    fs::write(data.join("index/nested/two"), [0; 13]).unwrap();
    let disk = scan_disk_use(&[
        DiskLocation::new(DiskKind::ListingCache, data.join("cache.db")),
        DiskLocation::new(DiskKind::Index, data.join("index")),
    ]);
    assert_eq!((disk.total.files, disk.total.bytes), (3, 31));
    assert_eq!(disk.locations[1].usage.bytes, 24);
    assert_eq!(disk.total.state, DiskState::Complete);
    assert_eq!(
        disk.locations[0].path,
        data.join("cache.db"),
        "the local view keeps the real path"
    );
}

#[test]
fn file_cap_marks_the_location_and_total_as_lower_bounds() {
    let root = tempfile::tempdir().unwrap();
    for name in ["one", "two", "three"] {
        fs::write(root.path().join(name), [0; 5]).unwrap();
    }
    let disk = scan_with_limit(
        &[DiskLocation::new(DiskKind::Index, root.path().to_owned())],
        2,
    );
    assert_eq!((disk.total.files, disk.total.bytes), (2, 10));
    assert_eq!(disk.locations[0].usage.state, DiskState::AtLeast);
    assert_eq!(disk.total.state, DiskState::AtLeast);
}

#[cfg(unix)]
#[test]
fn skips_symlinked_files_folders_and_location_ancestors() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret"), [0; 19]).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret"), root.path().join("file")).unwrap();
    for path in [root.path().to_owned(), root.path().join("link/secret")] {
        let disk = scan_disk_use(&[DiskLocation::new(DiskKind::Index, path)]);
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
    let disk = scan_disk_use(&[DiskLocation::new(
        DiskKind::PerformanceRecordings,
        root.path().to_owned(),
    )]);
    assert_eq!((disk.total.files, disk.total.bytes), (1, 9));
}

#[test]
fn sqlite_side_files_are_one_location_with_its_parts() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("cache.db");
    let wal = root.path().join("cache.db-wal");
    let shm = root.path().join("cache.db-shm");
    fs::write(&db, [0; 7]).unwrap();
    fs::write(&wal, [0; 11]).unwrap();
    fs::write(&shm, [0; 13]).unwrap();
    let disk = scan_disk_use(&[DiskLocation::sqlite(DiskKind::ListingCache, db.clone())]);
    let location = &disk.locations[0];
    assert_eq!(disk.locations.len(), 1);
    assert_eq!((location.usage.files, location.usage.bytes), (3, 31));
    assert_eq!(disk.total.bytes, 31);
    let parts: Vec<(PathBuf, u64)> = location
        .parts
        .iter()
        .map(|part| (part.path.clone(), part.usage.bytes))
        .collect();
    assert_eq!(parts, [(wal, 11), (shm, 13)]);
}

#[test]
fn sqlite_group_without_side_files_has_no_parts() {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("navo.db");
    fs::write(&db, [0; 5]).unwrap();
    let disk = scan_disk_use(&[DiskLocation::sqlite(DiskKind::Journal, db)]);
    let location = &disk.locations[0];
    assert_eq!((location.usage.files, location.usage.bytes), (1, 5));
    assert!(location.parts.is_empty());
}

#[test]
fn a_folder_denied_by_the_os_is_not_readable_not_a_lower_bound() {
    let index = fake_folder("blocked/index");
    let mut reader = FakeReader::default();
    reader.folders.insert(index.clone(), Vec::new());
    reader.denied.insert(index.clone());
    let disk = scan_with_reader(
        &[DiskLocation::new(DiskKind::Index, index)],
        FILE_LIMIT,
        &reader,
    );
    assert_eq!(disk.locations[0].usage.state, DiskState::NotReadable);
    assert_eq!(disk.locations[0].usage.bytes, 0);
    assert_eq!(disk.total.state, DiskState::AtLeast);
}
