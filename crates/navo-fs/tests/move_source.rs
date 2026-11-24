//! A move removes a copied source only while it is still the file that was copied: these tests
//! replace the source on a real disk (NTFS on Windows) between its copy and its removal.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on a broken fixture"
)]

mod support;

use std::fs;
use std::path::Path;

use navo_core::{FsErrorKind, Location};
use navo_fs::{CancelToken, CopiedSource, CopyControl, LocalFs, MoveCopy, ProviderOps};
use support::location_of;

/// Copies `at` beside itself as a move does and returns the source as the copy read it.
fn copy_moved(ops: &LocalFs, at: &Location, source: &Path) -> CopiedSource {
    let target = location_of(&source.with_extension("copy"));
    let request = MoveCopy {
        source: at,
        target: &target,
        created: None,
    };
    let control = CopyControl {
        progress: &mut |_| {},
        cancel: &CancelToken::new(),
    };
    let (_, read) = ops.copy_moved_file(request, control).unwrap();
    read.expect("a local copy proves the source it read")
}

/// Puts a new file with the same size and modified time over `source`, as an editor's save does.
fn replace_with_twin(source: &Path) {
    let modified = fs::metadata(source).unwrap().modified().unwrap();
    let twin = source.with_extension("twin");
    fs::write(&twin, b"replaced").unwrap();
    let file = fs::File::options().write(true).open(&twin).unwrap();
    file.set_modified(modified).unwrap();
    drop(file);
    fs::rename(&twin, source).unwrap();
}

#[test]
fn a_moved_source_replaced_after_its_copy_is_kept() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.txt");
    fs::write(&source, b"original").unwrap();
    let (ops, at) = (LocalFs::new(), location_of(&source));
    let expected = copy_moved(&ops, &at, &source);
    assert!(expected.identity.is_some(), "this volume gives no file ID");
    replace_with_twin(&source);
    let error = ops.remove_copied_source(&at, expected).unwrap_err();
    assert_eq!(error.kind, FsErrorKind::ChangedOnDisk);
    assert_eq!(fs::read(&source).unwrap(), b"replaced");
}

#[test]
fn an_unchanged_moved_source_is_removed() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.txt");
    fs::write(&source, b"original").unwrap();
    let (ops, at) = (LocalFs::new(), location_of(&source));
    let expected = copy_moved(&ops, &at, &source);
    ops.remove_copied_source(&at, expected).unwrap();
    assert!(!source.exists());
}

#[cfg(windows)]
#[test]
fn an_unchanged_read_only_moved_source_is_removed() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.txt");
    fs::write(&source, b"original").unwrap();
    let mut permissions = fs::metadata(&source).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&source, permissions).unwrap();
    let (ops, at) = (LocalFs::new(), location_of(&source));
    let expected = copy_moved(&ops, &at, &source);
    ops.remove_copied_source(&at, expected).unwrap();
    assert!(!source.exists());
}

#[test]
fn an_empty_moved_source_is_proved_and_removed() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("empty.txt");
    fs::write(&source, b"").unwrap();
    let (ops, at) = (LocalFs::new(), location_of(&source));
    let expected = copy_moved(&ops, &at, &source);
    assert!(
        expected.proves(),
        "an empty file still needs a proof to be moved"
    );
    ops.remove_copied_source(&at, expected).unwrap();
    assert!(!source.exists());
}

#[cfg(windows)]
#[test]
fn a_moved_source_open_for_writing_is_kept_as_in_use() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.txt");
    fs::write(&source, b"original").unwrap();
    let (ops, at) = (LocalFs::new(), location_of(&source));
    let expected = copy_moved(&ops, &at, &source);
    let writer = fs::File::options().write(true).open(&source).unwrap();
    let error = ops.remove_copied_source(&at, expected).unwrap_err();
    drop(writer);
    assert_eq!(error.kind, FsErrorKind::SharingViolation);
    assert_eq!(fs::read(&source).unwrap(), b"original");
}
