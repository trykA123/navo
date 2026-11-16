//! Parity tests: `StdSide` (plain `std::fs`) lists and stamps a tree as `LocalSide` (`LocalFs`) does.

use std::fs;
use std::io;
use std::ops::ControlFlow;
use std::path::Path;

use navo_compare::model::{CancelToken, ReadHint, Reason, RelPath, SideEntry, SideSource};
use navo_core::ProviderPath;

use super::{location, write};
use crate::compare::handle::{CompareTarget, open_side, std_local};
use crate::compare::local::LocalSide;
use crate::compare::std_side::StdSide;

/// Every entry of `folder` on `side`, sorted by row path.
fn list(side: &dyn SideSource, folder: &RelPath) -> Vec<SideEntry> {
    let mut found = Vec::new();
    side.list(
        folder,
        &mut |entry| {
            found.push(entry);
            ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )
    .unwrap();
    found.sort_by_key(|entry| entry.rel.as_path().to_text());
    found
}

/// Creates a link at `at` pointing to `target`; on Windows `folder` picks the link type.
fn link(target: &Path, at: &Path, folder: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        let _ = folder;
        std::os::unix::fs::symlink(target, at)
    }
    #[cfg(windows)]
    {
        if folder {
            std::os::windows::fs::symlink_dir(target, at)
        } else {
            std::os::windows::fs::symlink_file(target, at)
        }
    }
}

/// The F2a tree plus a hidden dot file, an empty folder and, on Unix, a name that is not UTF-8.
fn tree(root: &Path) {
    write(root, "file.txt", b"12345");
    write(root, "folder/inner.txt", b"abc");
    write(root, ".hidden", b"h");
    fs::create_dir(root.join("empty")).unwrap();
    // Windows without Developer Mode refuses links; the rest of the tree still compares.
    let _ = link(&root.join("file.txt"), &root.join("to_file"), false)
        .and_then(|()| link(&root.join("folder"), &root.join("to_folder"), true));
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"bad\xff.txt");
        fs::write(root.join(name), b"xy").unwrap();
    }
}

/// Asserts both sides list `folder` alike and give equal stamps for every entry.
fn assert_same_folder(std: &StdSide, local: &LocalSide, folder: &RelPath) -> Vec<SideEntry> {
    let entries = list(local, folder);
    assert_eq!(list(std, folder), entries, "listing of {folder:?}");
    for entry in &entries {
        let (a, b) = (std.stamp(&entry.rel), local.stamp(&entry.rel));
        let a = a.map(|s| (s.size, s.modified, s.file_id));
        let b = b.map(|s| (s.size, s.modified, s.file_id));
        assert_eq!(a, b, "stamp of {:?}", entry.rel);
        assert_eq!(std.file_id(&entry.rel), local.file_id(&entry.rel));
        assert_eq!(std.cache_path(&entry.rel), local.cache_path(&entry.rel));
    }
    entries
}

#[test]
fn std_side_lists_like_local_side() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let std = StdSide::new(location(dir.path())).unwrap();
    let local = LocalSide::new(location(dir.path())).unwrap();
    let top = assert_same_folder(&std, &local, &RelPath::root());
    assert!(top.len() >= 5, "listing too short: {top:?}");
    assert_same_folder(
        &std,
        &local,
        &RelPath::new(ProviderPath::from_text("folder")),
    );
    assert_same_folder(
        &std,
        &local,
        &RelPath::new(ProviderPath::from_text("empty")),
    );
    assert_eq!(std.provider(), local.provider());
    assert_eq!(std.volume(&RelPath::root()), local.volume(&RelPath::root()));
    let flags = |s: &dyn SideSource| (s.case_sensitive(), s.read_only(), s.is_network());
    assert_eq!(flags(&std), flags(&local));
}

#[test]
fn std_side_fails_like_local_side() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"abc");
    let std = StdSide::new(location(dir.path())).unwrap();
    let gone = RelPath::new(ProviderPath::from_text("gone.txt"));
    assert_eq!(std.stamp(&gone).unwrap_err(), Reason::NotFound);
    assert!(matches!(
        std.open(&gone, ReadHint::Buffered),
        Err(Reason::NotFound)
    ));
    let file = location(&dir.path().join("a.txt"));
    assert!(matches!(StdSide::new(file), Err(Reason::Unsupported)));
    let missing = location(&dir.path().join("missing"));
    assert!(matches!(StdSide::new(missing), Err(Reason::NotFound)));
}

#[test]
fn a_root_run_lists_the_children_but_not_the_root() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"abc");
    fs::create_dir(dir.path().join("sub")).unwrap();
    let side = StdSide::new(location(dir.path())).unwrap();
    let names: Vec<_> = list(&side, &RelPath::root())
        .iter()
        .map(|entry| entry.rel.as_path().to_text())
        .collect();
    assert_eq!(names, ["a.txt", "sub"]);
}

#[test]
fn a_cancelled_std_list_stops_without_entries() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"abc");
    let side = StdSide::new(location(dir.path())).unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    let mut seen = 0;
    let listed = side.list(
        &RelPath::root(),
        &mut |_| {
            seen += 1;
            ControlFlow::Continue(())
        },
        &cancel,
    );
    assert_eq!((listed, seen), (Ok(()), 0));
}

#[test]
fn the_glue_default_opens_a_std_side() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let target = std_local()(&location(dir.path()));
    assert!(
        matches!(target, CompareTarget::Local(_)),
        "default is not a plain local target"
    );
    let side = open_side(target, &mut Vec::new()).unwrap();
    let direct = StdSide::new(location(dir.path())).unwrap();
    assert_eq!(side.provider(), direct.provider());
    let top = list(&*side, &RelPath::root());
    assert_eq!(top, list(&direct, &RelPath::root()));
    assert!(top.len() >= 5, "listing too short: {top:?}");
}
