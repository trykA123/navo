use std::fs;
use std::ops::ControlFlow;
use std::path::Path;

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};
use tempfile::TempDir;

use super::*;
use crate::{EntryBatch, EntryKind};

#[cfg(windows)]
mod copy_windows;
mod errors;
mod hostile;
mod listing;
mod metadata;
#[cfg(windows)]
mod native;
mod notify_info;
mod ops;
mod pause;
mod perf;
mod verbatim;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
fn location_of(path: &Path) -> Location {
    use std::os::unix::ffi::OsStrExt;
    Location::local(ProviderPath::from_bytes(path.as_os_str().as_bytes())).unwrap()
}

#[cfg(windows)]
fn location_of(path: &Path) -> Location {
    use std::os::windows::ffi::OsStrExt;
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    Location::local(ProviderPath::from_wide(&units)).unwrap()
}

fn temp() -> (TempDir, Location) {
    let dir = tempfile::tempdir().unwrap();
    let location = location_of(dir.path());
    (dir, location)
}

fn list_with(fs: &LocalFs, folder: &Location) -> (Result<(), FsError>, Vec<EntryBatch>) {
    let mut batches = Vec::new();
    let mut sink = |batch: EntryBatch| {
        batches.push(batch);
        ControlFlow::Continue(())
    };
    let result = fs.list(folder, &mut sink, &CancelToken::new());
    (result, batches)
}

fn list_all(folder: &Location) -> (Result<(), FsError>, Vec<EntryBatch>) {
    list_with(&LocalFs::new(), folder)
}

fn entries(batches: &[EntryBatch]) -> Vec<FsEntry> {
    let mut all: Vec<FsEntry> = batches.iter().flat_map(|b| b.entries.clone()).collect();
    all.sort_by_key(|e| e.name.to_text());
    all
}

fn sorted_names(batches: &[EntryBatch]) -> Vec<String> {
    entries(batches).iter().map(|e| e.name.to_text()).collect()
}

fn entry_named(folder: &Location, name: &str) -> FsEntry {
    entries(&list_all(folder).1)
        .into_iter()
        .find(|e| e.name.as_str() == Some(name))
        .unwrap()
}

fn shape(batches: &[EntryBatch]) -> Vec<(usize, bool)> {
    batches.iter().map(|b| (b.entries.len(), b.last)).collect()
}

fn kind_of<T>(result: Result<T, FsError>) -> Option<FsErrorKind> {
    result.err().map(|e| e.kind)
}

fn write_file(dir: &Path, name: &str, bytes: usize) {
    fs::write(dir.join(name), vec![b'x'; bytes]).unwrap();
}

fn with_files(count: usize) -> (TempDir, Location) {
    let (dir, location) = temp();
    for i in 0..count {
        write_file(dir.path(), &format!("f{i:02}"), 1);
    }
    (dir, location)
}

#[cfg(unix)]
fn running_as_root(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(dir).unwrap().uid() == 0
}

#[test]
fn id_is_local() {
    assert_eq!(LocalFs::new().id(), rusty_core::ProviderId::local());
}

#[test]
fn only_watching_is_on_where_a_watcher_exists() {
    let watch = cfg!(any(windows, target_os = "linux"));
    assert_eq!(
        LocalFs::new().capabilities(),
        Capabilities {
            watch,
            ..Capabilities::default()
        }
    );
}

#[cfg(unix)]
mod unix_os_path {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn utf8_path_is_its_bytes() {
        let location = Location::local(ProviderPath::from_text("/tmp/a b")).unwrap();
        assert_eq!(os_path(&location).unwrap(), PathBuf::from("/tmp/a b"));
    }

    #[test]
    fn invalid_utf8_path_keeps_its_bytes() {
        let raw = [b'/', b't', b'/', 0xFF, b'x', 0xC0];
        let location = Location::local(ProviderPath::from_bytes(&raw)).unwrap();
        assert_eq!(
            os_path(&location).unwrap(),
            PathBuf::from(OsStr::from_bytes(&raw))
        );
    }
}
