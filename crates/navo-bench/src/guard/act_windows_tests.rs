#![allow(
    clippy::disallowed_methods,
    reason = "Windows write-API tests open competing handles in temp trees"
)]

use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};

use super::act::{Stage, seam};
use super::test_support::{TestRecycler, link, mkdir, rename, tree, write};
use super::{GuardError, PathRole, RecycleError, ScratchRoot, ShellRecycler};

const ROLE: PathRole = PathRole::ScratchRoot;
const SHARING_VIOLATION: i32 = 32;

fn open_for_delete(path: &Path) -> std::io::Result<fs::File> {
    OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

#[test]
fn large_junction_swapped_in_between_verify_and_act_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("owner-file"), b"owner bytes");
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
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
}

#[test]
fn medium_root_pin_refuses_a_second_handle_with_delete_access() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let error = open_for_delete(scratch.path()).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(SHARING_VIOLATION));
    let root = scratch.path().to_path_buf();
    drop(scratch);
    drop(open_for_delete(&root).unwrap());
}

#[test]
fn medium_verified_handle_shares_delete_with_the_act() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    write(&scratch.path().join("file"), b"bytes");
    let path = scratch.path().join("file");
    seam::set(Stage::BeforeAct, move || {
        drop(open_for_delete(&path).unwrap());
    });
    scratch
        .recycle(Path::new("file"), &TestRecycler::default())
        .unwrap();
    assert!(!scratch.path().join("file").exists());
}

#[test]
fn large_shell_recycler_moves_a_file_to_the_recycle_bin() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    write(&scratch.path().join("navo-bench-recycle-probe"), b"bytes");
    scratch
        .recycle(Path::new("navo-bench-recycle-probe"), &ShellRecycler)
        .unwrap();
    assert!(!scratch.path().join("navo-bench-recycle-probe").exists());
}

#[test]
fn large_shell_recycler_moves_the_whole_root() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    write(&scratch.path().join("data"), b"bytes");
    let root = scratch.path().to_path_buf();
    scratch.remove_root(&ShellRecycler).unwrap();
    assert!(!root.exists());
}

#[test]
fn small_shell_flags_recycle_without_any_dialog() {
    use windows::Win32::UI::Shell::{
        FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOFX_RECYCLEONDELETE,
    };
    let flags = super::shell_sink::OPERATION_FLAGS.0;
    for flag in [
        FOFX_RECYCLEONDELETE,
        FOF_ALLOWUNDO,
        FOF_NOCONFIRMATION,
        FOF_NOERRORUI,
        FOF_SILENT,
    ] {
        assert_eq!(flags & flag.0, flag.0);
    }
}

#[test]
fn large_shell_veto_refuses_an_item_with_another_identity() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    write(&scratch.path().join("kept"), b"bytes");
    write(&scratch.path().join("other"), b"other");
    let other = super::act_platform::identity_at(&scratch.path().join("other"))
        .unwrap()
        .unwrap();
    let kept = scratch.path().join("kept");
    let shell = kept
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();
    assert_eq!(
        super::shell_sink::recycle(Path::new(&shell), other).unwrap_err(),
        RecycleError::OtherItem
    );
    assert_eq!(fs::read(&kept).unwrap(), b"bytes");
}

fn try_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[test]
fn large_a_held_parent_cannot_be_swapped_during_a_create() {
    for stage in [Stage::ParentHeld, Stage::AfterCreate] {
        let parent = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let scratch = ScratchRoot::create(parent.path(), None).unwrap();
        mkdir(&scratch.path().join("sub"));
        let (from, to) = (scratch.path().join("sub"), outside.path().join("sub"));
        seam::set(stage, move || {
            assert!(
                try_rename(&from, &to).is_err(),
                "a held parent must not move"
            );
        });
        scratch.create_dir(Path::new("sub/new")).unwrap();
        assert!(scratch.path().join("sub/new").is_dir());
        assert!(tree(outside.path()).is_empty());
    }
}

#[test]
fn large_held_parents_and_the_root_pin_last_until_the_recycle() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    mkdir(&scratch.path().join("sub"));
    write(&scratch.path().join("sub/item"), b"bytes");
    let (from, to) = (scratch.path().join("sub"), outside.path().join("sub"));
    seam::set(Stage::BeforeAct, move || {
        assert!(
            try_rename(&from, &to).is_err(),
            "a held parent must not move"
        );
    });
    scratch
        .recycle(Path::new("sub/item"), &TestRecycler::default())
        .unwrap();
    let root = scratch.path().to_path_buf();
    let (root_check, container) = (root.clone(), parent.path().to_path_buf());
    let renamed = outside.path().join("renamed");
    seam::set(Stage::BeforeAct, move || {
        let error = open_for_delete(&root_check).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(SHARING_VIOLATION));
        assert!(
            try_rename(&container, &renamed).is_err(),
            "the container is held"
        );
    });
    scratch.remove_root(&ShellRecycler).unwrap();
    assert!(!root.exists());
}

#[test]
fn large_shell_veto_refuses_a_junction_even_with_its_own_identity() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    write(&outside.path().join("owner-file"), b"owner bytes");
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let junction = scratch.path().join("junction");
    link(outside.path(), &junction);
    let identity = super::act_platform::identity(
        &super::act_platform::open_attributes(&junction).unwrap(),
        ROLE,
    )
    .unwrap();
    let shell = junction
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();
    assert_eq!(
        super::shell_sink::recycle(Path::new(&shell), identity).unwrap_err(),
        RecycleError::OtherItem
    );
    assert_eq!(
        fs::read(outside.path().join("owner-file")).unwrap(),
        b"owner bytes"
    );
}
