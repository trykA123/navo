use std::fs;
use std::path::Path;

use super::act::{Stage, seam};
use super::test_support::{
    AlwaysFails, Liar, TestRecycler, binned, link, mkdir, remove, rename, tree, write,
};
use super::{GuardError, MARKER_FILE, PathRole, RecycleError, ScratchRoot};

const ROLE: PathRole = PathRole::ScratchRoot;

fn root() -> (tempfile::TempDir, ScratchRoot) {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    (parent, scratch)
}

#[test]
fn medium_recycle_moves_and_never_deletes() {
    let (_parent, scratch) = root();
    write(&scratch.path().join("file"), b"kept bytes");
    mkdir(&scratch.path().join("dir/inner"));
    write(&scratch.path().join("dir/inner/leaf"), b"leaf");
    let recycler = TestRecycler::default();
    scratch.recycle(Path::new("file"), &recycler).unwrap();
    scratch.recycle(Path::new("dir"), &recycler).unwrap();
    assert!(!scratch.path().join("file").exists());
    assert!(!scratch.path().join("dir").exists());
    assert_eq!(
        fs::read(binned(scratch.path(), "file")).unwrap(),
        b"kept bytes"
    );
    assert_eq!(
        fs::read(binned(scratch.path(), "dir").join("inner/leaf")).unwrap(),
        b"leaf"
    );
}

#[test]
fn medium_failed_recycle_leaves_the_item() {
    let (_parent, scratch) = root();
    write(&scratch.path().join("file"), b"kept bytes");
    let recycler = AlwaysFails::default();
    assert_eq!(
        scratch.recycle(Path::new("file"), &recycler).unwrap_err(),
        GuardError::Recycle(ROLE, RecycleError::Shell)
    );
    assert_eq!(recycler.calls.get(), 1);
    assert_eq!(
        fs::read(scratch.path().join("file")).unwrap(),
        b"kept bytes"
    );
}

#[test]
fn medium_recycler_claiming_success_without_moving_is_refused() {
    let (_parent, scratch) = root();
    write(&scratch.path().join("file"), b"kept bytes");
    assert_eq!(
        scratch.recycle(Path::new("file"), &Liar).unwrap_err(),
        GuardError::Recycle(ROLE, RecycleError::StillPresent)
    );
    assert!(scratch.path().join("file").exists());
}

#[test]
fn medium_recycle_refuses_outside_marker_and_missing_items() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("victim"), b"owner bytes");
    link(outside.path(), &scratch.path().join("out"));
    link(
        &outside.path().join("victim"),
        &scratch.path().join("file-link"),
    );
    let before = tree(outside.path());
    let recycler = TestRecycler::default();
    let refused = [
        ("out", GuardError::ReparsePoint(ROLE)),
        ("out/victim", GuardError::ReparsePoint(ROLE)),
        ("file-link", GuardError::ReparsePoint(ROLE)),
        ("../x", GuardError::InvalidPath(ROLE)),
        (MARKER_FILE, GuardError::InvalidPath(ROLE)),
        ("", GuardError::InvalidPath(ROLE)),
        ("missing", GuardError::Missing(ROLE)),
    ];
    for (name, error) in refused {
        assert_eq!(
            scratch.recycle(Path::new(name), &recycler).unwrap_err(),
            error
        );
    }
    assert_eq!(recycler.calls.get(), 0);
    assert_eq!(tree(outside.path()), before);
    assert!(scratch.path().join(MARKER_FILE).exists());
}

#[test]
fn large_directory_swapped_for_a_link_between_verify_and_act_is_refused() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("owner-file"), b"owner bytes");
    mkdir(&scratch.path().join("victim"));
    let before = tree(outside.path());
    let (victim, moved, target) = (
        scratch.path().join("victim"),
        scratch.path().join("moved"),
        outside.path().to_path_buf(),
    );
    seam::set(Stage::BeforeAct, move || {
        rename(&victim, &moved);
        link(&target, &victim);
    });
    let recycler = TestRecycler::default();
    assert_eq!(
        scratch.recycle(Path::new("victim"), &recycler).unwrap_err(),
        GuardError::Changed(ROLE)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert_eq!(tree(outside.path()), before);
    assert!(scratch.path().join("moved").is_dir());
}

#[test]
fn large_file_replaced_by_a_link_between_verify_and_act_is_refused() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("owner-file"), b"owner bytes");
    write(&scratch.path().join("victim"), b"scratch bytes");
    let before = tree(outside.path());
    let (victim, target) = (
        scratch.path().join("victim"),
        outside.path().join("owner-file"),
    );
    seam::set(Stage::BeforeAct, move || {
        remove(&victim);
        link(&target, &victim);
    });
    let recycler = TestRecycler::default();
    assert_eq!(
        scratch.recycle(Path::new("victim"), &recycler).unwrap_err(),
        GuardError::Changed(ROLE)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert_eq!(tree(outside.path()), before);
}

#[test]
fn large_parent_swapped_for_a_link_between_verify_and_act_is_refused() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("victim"), b"owner bytes");
    mkdir(&scratch.path().join("dir"));
    write(&scratch.path().join("dir/victim"), b"scratch bytes");
    let before = tree(outside.path());
    let (dir, moved, target) = (
        scratch.path().join("dir"),
        scratch.path().join("dir-moved"),
        outside.path().to_path_buf(),
    );
    seam::set(Stage::BeforeAct, move || {
        rename(&dir, &moved);
        link(&target, &dir);
    });
    let recycler = TestRecycler::default();
    assert_eq!(
        scratch
            .recycle(Path::new("dir/victim"), &recycler)
            .unwrap_err(),
        GuardError::Changed(ROLE)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert_eq!(tree(outside.path()), before);
}

#[test]
fn large_discard_after_a_mismatch_refuses_a_swapped_destination() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    write(&outside.path().join("owner-file"), b"owner bytes");
    write(&source.path().join("file"), b"source bytes");
    let before = tree(outside.path());
    let (dest, target) = (
        scratch.path().join("dest"),
        outside.path().join("owner-file"),
    );
    let corrupt = dest.clone();
    seam::set(Stage::BeforeHashCheck, move || {
        super::test_support::append(&corrupt, b"!")
    });
    seam::set(Stage::BeforeAct, move || {
        remove(&dest);
        link(&target, &dest);
    });
    assert_eq!(
        scratch
            .copy_in(&source.path().join("file"), Path::new("dest"))
            .unwrap_err(),
        GuardError::Mismatch(ROLE)
    );
    assert_eq!(tree(outside.path()), before);
}
