//! Tests for the `std::fs` compare walk: kinds, links, unreadable folders and cancel.

use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use navo_core::EntryKind;

use super::{WalkEntry, list_dir, stat};

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

/// Returns the entry named `name`, or panics with the listing.
fn named<'a>(entries: &'a [WalkEntry], name: &str) -> &'a WalkEntry {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("no {name} in {entries:?}"))
}

#[test]
fn a_tree_lists_files_folders_and_both_kinds_of_link() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("file.txt"), b"12345").unwrap();
    fs::create_dir(root.join("folder")).unwrap();
    // Windows without Developer Mode refuses links; the CI run there proves the rest.
    if link(&root.join("file.txt"), &root.join("to_file"), false)
        .and_then(|()| link(&root.join("folder"), &root.join("to_folder"), true))
        .and_then(|()| link(&root.join("gone"), &root.join("dangling"), false))
        .is_err()
    {
        return;
    }

    let entries = list_dir(root, &AtomicBool::new(false)).unwrap();

    assert_eq!(entries.len(), 5, "{entries:?}");
    let file = named(&entries, "file.txt");
    assert_eq!(
        (file.kind, file.size, file.link_to_folder),
        (EntryKind::File, 5, false)
    );
    assert!(file.modified_ms.is_some_and(|ms| ms > 0));
    let folder = named(&entries, "folder");
    assert_eq!(
        (folder.kind, folder.size, folder.link_to_folder),
        (EntryKind::Folder, 0, false)
    );
    let to_file = named(&entries, "to_file");
    assert_eq!(
        (to_file.kind, to_file.size, to_file.link_to_folder),
        (EntryKind::Link, 0, false)
    );
    let to_folder = named(&entries, "to_folder");
    assert_eq!(
        (to_folder.kind, to_folder.link_to_folder),
        (EntryKind::Link, true)
    );
    let dangling = named(&entries, "dangling");
    assert_eq!(
        (dangling.kind, dangling.link_to_folder),
        (EntryKind::Link, false)
    );

    for entry in &entries {
        assert_eq!(&stat(&root.join(&entry.name)).unwrap(), entry);
    }
}

#[test]
fn stat_of_a_missing_path_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let failure = stat(&dir.path().join("missing")).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn an_unreadable_folder_is_an_error_not_an_empty_list() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("inside.txt"), b"x").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads any folder, so the check only means something for a normal user.
    let readable_anyway = fs::read_dir(&locked).is_ok();

    let listed = list_dir(&locked, &AtomicBool::new(false));

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if readable_anyway {
        return;
    }
    let failure = listed.unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn a_cancelled_walk_is_interrupted() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), b"a").unwrap();

    let failure = list_dir(dir.path(), &AtomicBool::new(true)).unwrap_err();

    assert_eq!(failure.kind(), io::ErrorKind::Interrupted);
}
