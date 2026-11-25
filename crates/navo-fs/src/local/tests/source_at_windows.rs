//! A move's source proof read by path (`source_at`) must equal the proof `CopyFile2` took, and
//! must match nothing that is not that same plain file.

use std::fs::{File, FileTimes};
use std::os::windows::fs::{FileTimesExt, symlink_file};
use std::path::Path;
use std::time::Duration;

use super::ops::child;
use super::*;
use crate::local::owned_windows::source_at;
use crate::{CopiedSource, CopyControl, MoveCopy, ProviderOps};

/// Copies `source.bin` to `copy.bin` as a move does; returns the proof the copy took.
fn moved(dir: &Path) -> CopiedSource {
    fs::write(dir.join("source.bin"), b"moved source").unwrap();
    let root = location_of(dir);
    let request = MoveCopy {
        source: &child(&root, "source.bin"),
        target: &child(&root, "copy.bin"),
        created: None,
    };
    let control = CopyControl {
        progress: &mut |_| {},
        cancel: &CancelToken::new(),
    };
    let (_, proof) = LocalFs::new().copy_moved_file(request, control).unwrap();
    proof.expect("CopyFile2 reads a proof from its source handle")
}

fn without_id(proof: CopiedSource) -> CopiedSource {
    CopiedSource {
        identity: None,
        ..proof
    }
}

#[test]
fn the_proof_read_by_path_equals_the_copy_proof_with_and_without_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let proof = moved(dir.path());
    let read = source_at(&dir.path().join("source.bin")).unwrap();
    assert_eq!(read, proof);
    assert!(proof.still(&read));
    assert!(without_id(proof).proves());
    assert!(without_id(proof).still(&without_id(read)));
}

#[test]
fn a_link_a_folder_or_a_missing_path_proves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    moved(dir.path());
    fs::create_dir(dir.path().join("folder")).unwrap();
    assert_eq!(source_at(&dir.path().join("folder")), None);
    assert_eq!(source_at(&dir.path().join("missing.bin")), None);
    let link = dir.path().join("link.bin");
    // Creating symlinks needs Developer Mode or the privilege; without it the link case is skipped.
    if symlink_file(dir.path().join("source.bin"), &link).is_err() {
        return;
    }
    assert_eq!(source_at(&link), None);
}

#[test]
fn a_replacement_with_the_same_size_and_modified_time_does_not_match() {
    let dir = tempfile::tempdir().unwrap();
    let proof = moved(dir.path());
    let source = dir.path().join("source.bin");
    let modified = fs::metadata(&source).unwrap().modified().unwrap();
    let created = fs::metadata(&source).unwrap().created().unwrap();
    fs::remove_file(&source).unwrap();
    fs::write(&source, b"other source").unwrap();
    // Name tunneling may hand the new file the old creation time; set a different one so the
    // no-ID proof sees it, and keep the old modified time.
    let times = FileTimes::new()
        .set_modified(modified)
        .set_created(created - Duration::from_secs(3_600));
    File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_times(times)
        .unwrap();
    let read = source_at(&source).unwrap();
    assert_eq!(
        (read.size, read.modified_ms),
        (proof.size, proof.modified_ms)
    );
    assert!(!proof.still(&read));
    assert!(!without_id(proof).still(&without_id(read)));
}
