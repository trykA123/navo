use std::fs;
use std::path::{Path, PathBuf};

use super::act::{Stage, seam};
use super::placement::create_test_root;
use super::test_support::{
    AlwaysFails, TestRecycler, binned, link, mkdir, remove, rename, tree, write,
};
use super::{GuardError, MARKER_FILE, PathRole, ScratchRoot, StaleCleanup};

const ROLE: PathRole = PathRole::ScratchRoot;
const HEX: &str = "0123456789abcdef0123456789abcdef";

fn run_name(stamp: u64) -> String {
    format!("run-{stamp}-{HEX}")
}

fn marked(parent: &Path, stamp: u64) -> PathBuf {
    let scratch = create_test_root(parent, None, &run_name(stamp), &[]).unwrap();
    scratch.path().to_path_buf()
}

#[test]
fn medium_remove_root_with_wrong_token_deletes_nothing() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let root = scratch.path().to_path_buf();
    write(&root.join(MARKER_FILE), &[7_u8; 32]);
    write(&root.join("data"), b"run data");
    let recycler = TestRecycler::default();
    assert_eq!(
        scratch.remove_root(&recycler).unwrap_err(),
        GuardError::Marker(ROLE)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert_eq!(fs::read(root.join("data")).unwrap(), b"run data");
}

#[test]
fn medium_remove_root_without_marker_deletes_nothing() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let root = scratch.path().to_path_buf();
    remove(&root.join(MARKER_FILE));
    let recycler = TestRecycler::default();
    assert_eq!(
        scratch.remove_root(&recycler).unwrap_err(),
        GuardError::Marker(ROLE)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert!(root.is_dir());
}

#[test]
fn medium_remove_root_refuses_a_marker_link_to_the_right_token() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let root = scratch.path().to_path_buf();
    write(&outside.path().join("token"), scratch.token());
    remove(&root.join(MARKER_FILE));
    link(&outside.path().join("token"), &root.join(MARKER_FILE));
    let recycler = TestRecycler::default();
    assert_eq!(
        scratch.remove_root(&recycler).unwrap_err(),
        GuardError::Marker(ROLE)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert!(root.is_dir());
}

#[test]
fn medium_remove_root_recycles_only_its_own_root() {
    let parent = tempfile::tempdir().unwrap();
    let sibling = ScratchRoot::create(parent.path(), None).unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let foreign = parent.path().join(run_name(1));
    mkdir(&foreign);
    write(&scratch.path().join("data"), b"run data");
    let root = scratch.path().to_path_buf();
    let name = root.file_name().unwrap().to_string_lossy().into_owned();
    let others = (tree(sibling.path()), tree(&foreign));
    scratch.remove_root(&TestRecycler::default()).unwrap();
    assert!(!root.exists());
    assert_eq!(
        fs::read(binned(parent.path(), &name).join("data")).unwrap(),
        b"run data"
    );
    assert_eq!((tree(sibling.path()), tree(&foreign)), others);
}

#[test]
fn large_remove_root_swapped_between_verify_and_act_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("owner-file"), b"owner bytes");
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let before = tree(outside.path());
    let (root, moved, target) = (
        scratch.path().to_path_buf(),
        parent.path().join("moved"),
        outside.path().to_path_buf(),
    );
    seam::set(Stage::BeforeAct, move || {
        rename(&root, &moved);
        link(&target, &root);
    });
    let recycler = TestRecycler::default();
    assert_eq!(
        scratch.remove_root(&recycler).unwrap_err(),
        GuardError::Changed(ROLE)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert_eq!(tree(outside.path()), before);
    assert!(parent.path().join("moved").join(MARKER_FILE).exists());
}

/// Plants an unmarked run, a short marker, a link to a marked run and a file named like a run.
fn plant_unusable_runs(parent: &Path, outside: &Path) -> (PathBuf, PathBuf) {
    let unmarked = parent.join(run_name(50));
    mkdir(&unmarked);
    let short = marked(parent, 70);
    write(&short.join(MARKER_FILE), &[1_u8; 31]);
    link(&marked(outside, 10), &parent.join(run_name(60)));
    write(&parent.join(run_name(40)), b"a file named like a run");
    (unmarked, short)
}

#[test]
fn large_clean_stale_runs_recycles_only_marked_older_runs() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let old = marked(parent.path(), 100);
    let newer = marked(parent.path(), 300);
    let current = create_test_root(parent.path(), None, &run_name(200), &[]).unwrap();
    let (unmarked, short) = plant_unusable_runs(parent.path(), outside.path());
    let foreign = [parent.path().join("run-abc"), parent.path().join("other")];
    for folder in &foreign {
        mkdir(folder);
    }
    let untouched = (
        tree(&newer),
        tree(&unmarked),
        tree(&short),
        tree(outside.path()),
    );
    let recycler = TestRecycler::default();
    let cleanup = current.clean_stale_runs(parent.path(), &recycler).unwrap();
    assert_eq!(
        cleanup,
        StaleCleanup {
            recycled: 1,
            refused: 4
        }
    );
    assert!(!old.exists());
    assert!(
        binned(parent.path(), &run_name(100))
            .join(MARKER_FILE)
            .exists()
    );
    assert_eq!(
        (
            tree(&newer),
            tree(&unmarked),
            tree(&short),
            tree(outside.path())
        ),
        untouched
    );
    assert!(foreign.iter().all(|folder| folder.is_dir()));
    assert!(current.path().join(MARKER_FILE).exists());
}

#[test]
fn medium_clean_stale_runs_refuses_another_parent() {
    let parent = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let old = marked(elsewhere.path(), 1);
    let current = ScratchRoot::create(parent.path(), None).unwrap();
    let recycler = TestRecycler::default();
    assert_eq!(
        current
            .clean_stale_runs(elsewhere.path(), &recycler)
            .unwrap_err(),
        GuardError::OutsideRoot(PathRole::StaleRun)
    );
    assert_eq!(recycler.calls.get(), 0);
    assert!(old.join(MARKER_FILE).exists());
}

#[test]
fn medium_clean_stale_runs_failure_leaves_the_run() {
    let parent = tempfile::tempdir().unwrap();
    let old = marked(parent.path(), 1);
    let current = ScratchRoot::create(parent.path(), None).unwrap();
    let recycler = AlwaysFails::default();
    let cleanup = current.clean_stale_runs(parent.path(), &recycler).unwrap();
    assert_eq!(
        cleanup,
        StaleCleanup {
            recycled: 0,
            refused: 1
        }
    );
    assert!(old.join(MARKER_FILE).exists());
}

#[test]
fn large_stale_run_swapped_between_verify_and_act_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("owner-file"), b"owner bytes");
    let old = marked(parent.path(), 1);
    let current = ScratchRoot::create(parent.path(), None).unwrap();
    let before = tree(outside.path());
    let (moved, target) = (parent.path().join("moved"), outside.path().to_path_buf());
    let victim = old.clone();
    seam::set(Stage::BeforeAct, move || {
        rename(&victim, &moved);
        link(&target, &victim);
    });
    let recycler = TestRecycler::default();
    let cleanup = current.clean_stale_runs(parent.path(), &recycler).unwrap();
    assert_eq!(
        cleanup,
        StaleCleanup {
            recycled: 0,
            refused: 1
        }
    );
    assert_eq!(recycler.calls.get(), 0);
    assert_eq!(tree(outside.path()), before);
}
