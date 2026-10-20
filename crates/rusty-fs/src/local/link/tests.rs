use std::fs;
use std::path::PathBuf;
use std::sync::Barrier;

use tempfile::TempDir;

use crate::make_link;

pub(super) fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("report.txt");
    let target = directory.path().join("links");
    fs::write(&source, b"source contents").unwrap();
    fs::create_dir(&target).unwrap();
    (directory, source, target)
}

#[cfg(unix)]
const FIRST_NAME: &str = "report.txt";
#[cfg(windows)]
const FIRST_NAME: &str = "report.txt - Shortcut.lnk";

#[cfg(unix)]
#[test]
fn creates_symlink_to_source() {
    let (_directory, source, target) = fixture();
    let created = make_link(&target, &source).unwrap();
    assert_eq!(created, target.join("report.txt"));
    assert!(fs::symlink_metadata(&created).unwrap().is_symlink());
    assert_eq!(fs::read_link(created).unwrap(), source);
}

#[cfg(unix)]
#[test]
fn numbers_conflicting_symlink_names() {
    let (_directory, source, target) = fixture();
    for name in ["report.txt", "report.txt (2)", "report.txt (3)"] {
        let created = make_link(&target, &source).unwrap();
        assert_eq!(created, target.join(name));
        assert_eq!(fs::read_link(created).unwrap(), source);
    }
}

#[test]
fn leaves_existing_file_untouched() {
    let (_directory, source, target) = fixture();
    let existing = target.join(FIRST_NAME);
    fs::write(&existing, b"existing contents").unwrap();
    let created = make_link(&target, &source).unwrap();
    assert_ne!(created, existing);
    assert_eq!(fs::read(existing).unwrap(), b"existing contents");
}

#[test]
fn leaves_source_unchanged() {
    let (_directory, source, target) = fixture();
    let before = fs::metadata(&source).unwrap();
    make_link(&target, &source).unwrap();
    let after = fs::metadata(&source).unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"source contents");
    assert_eq!(after.len(), before.len());
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    assert_eq!(after.permissions(), before.permissions());
}

#[test]
fn concurrent_links_never_replace_existing_entries() {
    let (_directory, source, target) = fixture();
    let existing = target.join(FIRST_NAME);
    fs::write(&existing, b"existing contents").unwrap();
    let barrier = Barrier::new(8);
    let mut created = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    make_link(&target, &source).unwrap()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    created.sort();
    created.dedup();
    assert_eq!(created.len(), 8);
    assert_eq!(fs::read_dir(&target).unwrap().count(), 9);
    assert_eq!(fs::read(existing).unwrap(), b"existing contents");
    assert_eq!(fs::read(source).unwrap(), b"source contents");
}
