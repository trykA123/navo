use std::path::Path;

use super::act::{Stage, seam};
use super::test_support::{TestRecycler, link, mkdir, rename, tree, write};
use super::{GuardError, OutputFolder, PathRole, ScratchRoot};

const ROLE: PathRole = PathRole::ScratchRoot;

fn root() -> (tempfile::TempDir, ScratchRoot) {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    (parent, scratch)
}

/// After the create, `sub` moves to `outside` and a link to it takes its place.
fn swap_after_create(scratch: &ScratchRoot, outside: &Path, with_link: bool) {
    let (from, to) = (scratch.path().join("sub"), outside.join("sub"));
    seam::set(Stage::AfterCreate, move || {
        rename(&from, &to);
        if with_link {
            link(&to, &from);
        }
    });
}

#[test]
fn medium_a_new_file_that_lands_elsewhere_is_removed_through_its_handle() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    mkdir(&scratch.path().join("sub"));
    swap_after_create(&scratch, outside.path(), false);
    assert_eq!(
        scratch.create_new(Path::new("sub/new")).unwrap_err(),
        GuardError::Changed(ROLE)
    );
    assert!(!outside.path().join("sub/new").exists());
    assert_eq!(
        tree(outside.path()),
        vec![(outside.path().join("sub"), Vec::new())]
    );
}

#[test]
fn medium_a_new_folder_that_lands_elsewhere_is_removed_again() {
    let (_parent, scratch) = root();
    let outside = tempfile::tempdir().unwrap();
    mkdir(&scratch.path().join("sub"));
    swap_after_create(&scratch, outside.path(), true);
    assert_eq!(
        scratch.create_dir(Path::new("sub/new")).unwrap_err(),
        GuardError::Changed(ROLE)
    );
    assert!(!outside.path().join("sub/new").exists());
    assert_eq!(
        tree(outside.path()),
        vec![(outside.path().join("sub"), Vec::new())]
    );
}

/// Moves `sub` to `elsewhere` at `stage` and puts a link to `owner` in its place.
fn swap_for_link_at(stage: Stage, scratch: &ScratchRoot, elsewhere: &Path, owner: &Path) {
    let (from, moved, target) = (
        scratch.path().join("sub"),
        elsewhere.join("sub"),
        owner.to_path_buf(),
    );
    seam::set(stage, move || {
        rename(&from, &moved);
        link(&target, &from);
    });
}

#[test]
fn medium_a_fresh_owner_folder_behind_a_swapped_parent_is_never_removed() {
    let (_parent, scratch) = root();
    let owner = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    mkdir(&scratch.path().join("sub"));
    mkdir(&owner.path().join("new"));
    swap_for_link_at(Stage::AfterCreate, &scratch, elsewhere.path(), owner.path());
    let before = tree(owner.path());
    assert_eq!(
        scratch.create_dir(Path::new("sub/new")).unwrap_err(),
        GuardError::Changed(ROLE)
    );
    assert_eq!(tree(owner.path()), before);
    assert!(owner.path().join("new").is_dir());
    assert!(!elsewhere.path().join("sub/new").exists());
}

#[test]
fn medium_a_parent_swapped_after_it_was_held_gets_no_stray() {
    for dir in [false, true] {
        let (_parent, scratch) = root();
        let owner = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        write(&owner.path().join("kept"), b"owner bytes");
        mkdir(&scratch.path().join("sub"));
        swap_for_link_at(Stage::ParentHeld, &scratch, elsewhere.path(), owner.path());
        let before = tree(owner.path());
        let error = if dir {
            scratch.create_dir(Path::new("sub/new")).unwrap_err()
        } else {
            scratch.create_new(Path::new("sub/new")).unwrap_err()
        };
        assert_eq!(error, GuardError::Changed(ROLE));
        assert_eq!(tree(owner.path()), before);
        assert_eq!(
            tree(elsewhere.path()),
            vec![(elsewhere.path().join("sub"), Vec::new())]
        );
    }
}

#[test]
fn medium_a_stray_whose_name_was_taken_over_is_left_and_reported() {
    for dir in [false, true] {
        let (_parent, scratch) = root();
        let outside = tempfile::tempdir().unwrap();
        mkdir(&scratch.path().join("sub"));
        let (from, moved) = (scratch.path().join("sub"), outside.path().join("sub"));
        seam::set(Stage::AfterCreate, move || {
            rename(&from, &moved);
            rename(&moved.join("new"), &moved.join("ours"));
            write(&moved.join("new"), b"owner bytes");
        });
        let error = if dir {
            scratch.create_dir(Path::new("sub/new")).unwrap_err()
        } else {
            scratch.create_new(Path::new("sub/new")).unwrap_err()
        };
        assert_eq!(error, GuardError::Left(ROLE));
        assert_eq!(
            std::fs::read(outside.path().join("sub/new")).unwrap(),
            b"owner bytes"
        );
        assert!(outside.path().join("sub/ours").exists());
    }
}

#[test]
fn medium_a_parent_swapped_before_a_recycle_is_refused() {
    let (_parent, scratch) = root();
    let owner = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    mkdir(&scratch.path().join("sub"));
    write(&scratch.path().join("sub/item"), b"bytes");
    write(&owner.path().join("item"), b"owner bytes");
    swap_for_link_at(Stage::BeforeAct, &scratch, elsewhere.path(), owner.path());
    let before = tree(owner.path());
    let recycler = TestRecycler::default();
    assert_eq!(
        scratch
            .recycle(Path::new("sub/item"), &recycler)
            .unwrap_err(),
        GuardError::Changed(ROLE)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert_eq!(tree(owner.path()), before);
    assert!(elsewhere.path().join("sub/item").is_file());
}

#[test]
fn small_creates_without_a_swap_keep_their_item() {
    let (_parent, scratch) = root();
    drop(scratch.create_new(Path::new("file")).unwrap());
    scratch.create_dir(Path::new("dir")).unwrap();
    assert!(scratch.path().join("file").is_file());
    assert!(scratch.path().join("dir").is_dir());
}

#[test]
fn medium_an_output_parent_swapped_for_a_link_before_the_create_gets_nothing_outside() {
    let base = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    write(&outside.path().join("kept"), b"owner bytes");
    let parent = base.path().join("parent");
    mkdir(&parent);
    let (from, moved, target) = (
        parent.clone(),
        elsewhere.path().join("parent"),
        outside.path().to_path_buf(),
    );
    seam::set(Stage::BeforeCreate, move || {
        rename(&from, &moved);
        link(&target, &from);
    });
    let before = tree(outside.path());
    let error = OutputFolder::create(&parent.join("out"), None).unwrap_err();
    assert_eq!(error, GuardError::Changed(ROLE));
    assert_eq!(tree(outside.path()), before);
    assert_eq!(
        tree(elsewhere.path()),
        vec![(elsewhere.path().join("parent"), Vec::new())]
    );
}
