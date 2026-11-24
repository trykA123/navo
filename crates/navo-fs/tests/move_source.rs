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
use navo_fs::{CopiedSource, LocalFs, Provider, ProviderOps};
use support::location_of;

/// What a move records as the copy of `at` starts.
fn seen(ops: &LocalFs, at: &Location) -> CopiedSource {
    let entry = ops.metadata(at).unwrap();
    CopiedSource {
        identity: ops.file_identity(at).unwrap(),
        size: entry.size,
        modified_ms: entry.modified_ms,
    }
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
    let expected = seen(&ops, &at);
    assert!(expected.identity.is_some(), "this volume gives no file ID");
    fs::copy(&source, folder.path().join("copy.txt")).unwrap();
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
    let expected = seen(&ops, &at);
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
    let expected = seen(&ops, &at);
    ops.remove_copied_source(&at, expected).unwrap();
    assert!(!source.exists());
}
