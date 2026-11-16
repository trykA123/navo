//! Tests for the spool cleanup: the start-up sweep, its creation gate and its folder lock.

use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

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
fn a_sweep_of_a_profile_folder_deletes_only_spool_folders() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let dead = dead_owner(&data);
    for (folder, file) in [
        ("logs", "app.log"),
        ("crash", "dump.dmp"),
        ("12-notes", "n.txt"),
    ] {
        fs::create_dir_all(data.join(folder)).unwrap();
        fs::write(data.join(folder).join(file), b"user data").unwrap();
    }
    fs::write(data.join("keep.txt"), b"user data").unwrap();
    Sources::sweep_startup(Some(&data));
    assert!(!dead.exists(), "a dead owner folder is swept");
    for kept in [
        "logs/app.log",
        "crash/dump.dmp",
        "12-notes/n.txt",
        "keep.txt",
    ] {
        assert!(
            data.join(kept).exists(),
            "{kept}: only folders named like a spool owner may be swept"
        );
    }
}

#[test]
fn a_relative_spool_root_sweeps_nothing() {
    let dir = tempfile::Builder::new()
        .prefix("spool-relative-")
        .tempdir_in(".")
        .unwrap();
    let dead = dead_owner(dir.path());
    let relative = PathBuf::from(dir.path().file_name().unwrap());
    Sources::sweep_startup(Some(&relative));
    assert!(
        dead.exists(),
        "a relative root depends on the working folder, so it is refused"
    );
}

#[test]
fn a_spool_root_with_a_parent_step_sweeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let dead = dead_owner(&dir.path().join("spool"));
    fs::create_dir_all(dir.path().join("other")).unwrap();
    let stepped = dir.path().join("other").join("..").join("spool");
    Sources::sweep_startup(Some(&stepped));
    assert!(
        dead.exists(),
        "a root with `..` may name another folder, so it is refused"
    );
}

#[cfg(unix)]
#[test]
fn a_linked_spool_root_sweeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let dead = dead_owner(&dir.path().join("real"));
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(dir.path().join("real"), &link).unwrap();
    Sources::sweep_startup(Some(&link));
    assert!(
        dead.exists(),
        "a root that is a link may point anywhere, so it is refused"
    );
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

/// A dead owner folder under `root`, named the way `Owner::open` names one, with one file.
fn dead_owner(root: &Path) -> PathBuf {
    let dead = root.join("222-2");
    fs::create_dir_all(&dead).unwrap();
    fs::write(dead.join("x"), b"stale").unwrap();
    dead
}
