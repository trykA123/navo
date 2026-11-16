//! Tests for the spool cleanup: the start-up sweep, its creation gate and its folder lock.

use std::fs;
use std::fs::OpenOptions;

use navo_compare::model::CancelToken;

use super::{FakeSide, MIB, blob, entry, small, spool_files};
use crate::compare::diff::sources::{SPOOL_DIR, Sources, sweep, sweep_with};

#[test]
fn the_startup_sweep_cleans_the_folder_new_spools_go_to() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join(SPOOL_DIR);
    let stale = root.join("222-2");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("owner.lock"), b"").unwrap();
    let sources = Sources::new(Some(root.clone()));
    let fake = FakeSide::zeros(3 * MIB as u64);
    let entry = entry("big.bin", 3 * MIB as u64);
    let _spooled = sources
        .open_git_with(blob(&fake, &entry), &CancelToken::new(), small())
        .unwrap();
    assert_eq!(
        spool_files(&root).len(),
        1,
        "the blob spools under the root"
    );
    Sources::sweep_root(&root);
    assert!(!stale.exists(), "the start-up sweep cleans the same root");
    assert_eq!(spool_files(&root).len(), 1, "a live spool stays");
}

#[test]
fn the_startup_sweep_removes_only_unlocked_process_folders() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("111-1");
    let stale = dir.path().join("222-2");
    fs::create_dir_all(&live).unwrap();
    fs::create_dir_all(&stale).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(live.join("owner.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    fs::write(stale.join("owner.lock"), b"").unwrap();
    fs::write(stale.join("dead.spool"), b"stale").unwrap();
    sweep(dir.path());
    assert!(live.exists(), "a locked process keeps its spool");
    assert!(!stale.exists(), "an unlocked process is swept");
}

#[test]
fn the_sweep_skips_a_folder_whose_owner_is_still_creating_it() {
    let dir = tempfile::tempdir().unwrap();
    let creating = dir.path().join("333-3");
    fs::create_dir_all(&creating).unwrap();
    let gate = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.path().join("sweep.lock"))
        .unwrap();
    gate.try_lock_shared().unwrap();
    sweep(dir.path());
    assert!(
        creating.exists(),
        "an owner between create and lock keeps its folder"
    );
}

#[test]
fn the_sweep_holds_the_folder_lock_until_the_folder_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let stale = dir.path().join("222-2");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("owner.lock"), b"").unwrap();
    let mut second_handle_locked = None;
    sweep_with(dir.path(), &mut |folder| {
        let other = OpenOptions::new()
            .read(true)
            .write(true)
            .open(folder.join("owner.lock"))
            .unwrap();
        second_handle_locked = Some(other.try_lock().is_ok());
    });
    assert_eq!(
        second_handle_locked,
        Some(false),
        "no other process may take the folder while the sweep deletes it"
    );
    assert!(!stale.exists());
}

#[test]
fn a_sweep_never_deletes_outside_its_spool_root() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join(SPOOL_DIR);
    let dead = spool.join("222-2");
    fs::create_dir_all(&dead).unwrap();
    fs::write(dead.join("x"), b"stale").unwrap();
    fs::write(dir.path().join("keep.txt"), b"user file").unwrap();
    fs::create_dir_all(dir.path().join("other")).unwrap();
    fs::write(dir.path().join("other").join("y"), b"user file").unwrap();
    Sources::sweep_root(&spool);
    assert!(!dead.exists(), "a dead owner inside the root is swept");
    assert!(
        dir.path().join("other").join("y").exists(),
        "a folder beside the spool root is never swept"
    );
    assert!(
        dir.path().join("keep.txt").exists(),
        "a file beside the root stays"
    );
    assert!(spool.is_dir(), "the spool root itself stays");
}

#[test]
fn a_host_without_a_spool_sweeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let dead = dir.path().join(SPOOL_DIR).join("222-2");
    fs::create_dir_all(&dead).unwrap();
    Sources::sweep_startup(None);
    assert!(dead.exists(), "no spool root means no sweep");
    Sources::sweep_startup(Some(&dir.path().join(SPOOL_DIR)));
    assert!(
        !dead.exists(),
        "the same folder is swept once the host names its root"
    );
}
