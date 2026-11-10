// Windows tests for the chain pin: holds, rechecks, and cautious cleanup.

#![cfg(windows)]

use std::path::Path;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SHARING_VIOLATION, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, OPEN_EXISTING,
};

use super::{Pin, PinError, ancestors_unchanged};
use crate::placement::{Chain, FileId};
use crate::probe::chain::resolve_path;

/// Builds the chain for `base\missing...`, where only `base` and its ancestors exist.
fn chain_under(base: &Path, missing: &[&str]) -> Chain {
    let mut path = base.to_path_buf();
    for name in missing {
        path.push(name);
    }
    resolve_path(&path).expect("chain resolves")
}

/// The identity of an existing path.
fn id_of(path: &Path) -> FileId {
    resolve_path(path)
        .expect("chain resolves")
        .existing
        .last()
        .expect("path exists")
        .id
}

/// Opens a directory with exclusive sharing, so a second open must fail.
fn open_exclusive(path: &Path) -> HANDLE {
    let path = wide_z(path);
    // SAFETY: the wide path is NUL-terminated and no security attributes are used.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            // Attribute-only opens skip sharing checks; listing access makes share 0 bite.
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(handle, INVALID_HANDLE_VALUE, "exclusive open failed");
    handle
}

/// A NUL-terminated UTF-16 path for `CreateFileW`.
fn wide_z(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[test]
fn renaming_a_held_root_or_parent_fails() {
    let temp = tempfile::tempdir().expect("temp folder");
    let chain = chain_under(temp.path(), &["a", "b"]);
    let _pin = Pin::fresh(&chain).expect("fresh pin");
    assert!(std::fs::rename(temp.path().join("a").join("b"), temp.path().join("moved-b")).is_err());
    assert!(std::fs::rename(temp.path().join("a"), temp.path().join("moved-a")).is_err());
}

#[test]
fn a_renamed_existing_ancestor_is_caught() {
    let temp = tempfile::tempdir().expect("temp folder");
    let parent = temp.path().join("parent");
    std::fs::create_dir(&parent).expect("parent");
    let chain = chain_under(temp.path(), &["parent", "new"]);
    assert!(ancestors_unchanged(&chain));
    std::fs::rename(&parent, temp.path().join("moved")).expect("rename parent");
    assert!(!ancestors_unchanged(&chain));
}

#[test]
fn an_identity_mismatch_refuses() {
    let temp = tempfile::tempdir().expect("temp folder");
    let id = id_of(temp.path());
    let wrong = FileId {
        volume: id.volume,
        id: id.id ^ 1,
    };
    assert!(matches!(
        Pin::existing(temp.path(), wrong),
        Err(PinError::Identity)
    ));
}

#[test]
fn a_sharing_violation_at_open_refuses() {
    let temp = tempfile::tempdir().expect("temp folder");
    let id = id_of(temp.path());
    let first = open_exclusive(temp.path());
    let result = Pin::existing(temp.path(), id);
    // SAFETY: `first` is a live handle returned by `open_exclusive`.
    unsafe { CloseHandle(first) };
    let sharing = i32::try_from(ERROR_SHARING_VIOLATION).expect("error code fits i32");
    assert!(matches!(
        result,
        Err(PinError::Io(error)) if error.raw_os_error() == Some(sharing)
    ));
}

#[test]
fn a_second_open_with_share_read_and_write_succeeds() {
    let temp = tempfile::tempdir().expect("temp folder");
    let id = id_of(temp.path());
    let first = Pin::existing(temp.path(), id).expect("first pin");
    let second = Pin::existing(temp.path(), id);
    assert!(second.is_ok());
    assert!(first.unchanged());
}

#[test]
fn remove_created_deletes_empty_folders_deepest_first() {
    let temp = tempfile::tempdir().expect("temp folder");
    let chain = chain_under(temp.path(), &["a", "b"]);
    let pin = Pin::fresh(&chain).expect("fresh pin");
    let removal = pin.remove_created();
    assert_eq!(
        removal.removed,
        vec![temp.path().join("a").join("b"), temp.path().join("a"),]
    );
    assert!(removal.kept.is_empty());
    assert!(!temp.path().join("a").exists());
}

#[test]
fn remove_created_keeps_a_root_with_a_file_and_its_parent() {
    let temp = tempfile::tempdir().expect("temp folder");
    let chain = chain_under(temp.path(), &["a", "b"]);
    let pin = Pin::fresh(&chain).expect("fresh pin");
    let root = temp.path().join("a").join("b");
    let parent = temp.path().join("a");
    std::fs::write(root.join("file.txt"), b"x").expect("file");
    let removal = pin.remove_created();
    assert!(removal.removed.is_empty());
    assert_eq!(removal.kept, vec![root.clone(), parent.clone()]);
    assert!(root.join("file.txt").exists());
    assert!(parent.exists());
}

#[test]
fn remove_created_keeps_folders_it_did_not_create() {
    let temp = tempfile::tempdir().expect("temp folder");
    let keep = temp.path().join("keep");
    std::fs::create_dir(&keep).expect("keep");
    let chain = chain_under(temp.path(), &["keep", "new"]);
    let pin = Pin::fresh(&chain).expect("fresh pin");
    let removal = pin.remove_created();
    assert_eq!(removal.removed, vec![keep.join("new")]);
    assert!(keep.exists());
}

#[test]
fn a_swapped_empty_folder_is_kept_by_identity() {
    let temp = tempfile::tempdir().expect("temp folder");
    let chain = chain_under(temp.path(), &["a"]);
    let mut pin = Pin::fresh(&chain).expect("fresh pin");
    pin.close_handles();
    let folder = temp.path().join("a");
    std::fs::remove_dir(&folder).expect("remove created folder");
    std::fs::create_dir(&folder).expect("recreate folder");
    let removal = pin.remove_created();
    assert!(removal.removed.is_empty());
    assert_eq!(removal.kept, vec![folder.clone()]);
    assert!(folder.exists());
}

#[test]
fn an_existing_leftover_root_is_held() {
    let temp = tempfile::tempdir().expect("temp folder");
    std::fs::write(temp.path().join("Update.exe"), b"x").expect("update");
    let chain = chain_under(temp.path(), &[]);
    let pin = Pin::fresh(&chain).expect("leftover root is held");
    assert_eq!(pin.root(), Some(temp.path()));
    assert!(!pin.created_root());
}

#[test]
fn a_leftover_root_with_a_subfolder_refuses() {
    let temp = tempfile::tempdir().expect("temp folder");
    std::fs::write(temp.path().join("Update.exe"), b"x").expect("update");
    std::fs::create_dir(temp.path().join("empty")).expect("empty");
    let chain = chain_under(temp.path(), &[]);
    assert!(matches!(Pin::fresh(&chain), Err(PinError::RootExists)));
}

#[test]
fn a_leftover_root_whose_identity_moved_refuses() {
    let temp = tempfile::tempdir().expect("temp folder");
    std::fs::write(temp.path().join("Update.exe"), b"x").expect("update");
    let mut chain = chain_under(temp.path(), &[]);
    let root = chain.existing.last_mut().expect("the root exists");
    root.id = FileId {
        volume: root.id.volume,
        id: root.id.id ^ 1,
    };
    assert!(matches!(Pin::fresh(&chain), Err(PinError::Identity)));
}

#[test]
fn remove_created_of_a_held_leftover_root_deletes_nothing() {
    let temp = tempfile::tempdir().expect("temp folder");
    std::fs::write(temp.path().join("Update.exe"), b"x").expect("update");
    let chain = chain_under(temp.path(), &[]);
    let pin = Pin::fresh(&chain).expect("leftover root is held");
    let removal = pin.remove_created();
    assert!(removal.removed.is_empty(), "{removal:?}");
    assert!(removal.kept.is_empty(), "{removal:?}");
    assert!(temp.path().exists(), "the existing root survives");
    assert!(temp.path().join("Update.exe").exists());
}

#[test]
fn remove_created_keeps_an_existing_root_and_its_update_exe() {
    let temp = tempfile::tempdir().expect("temp folder");
    let keep = temp.path().join("keep");
    std::fs::create_dir(&keep).expect("keep");
    std::fs::write(keep.join("Update.exe"), b"x").expect("update");
    let chain = chain_under(temp.path(), &["keep", "new"]);
    let pin = Pin::fresh(&chain).expect("fresh pin");
    let removal = pin.remove_created();
    assert_eq!(removal.removed, vec![keep.join("new")]);
    assert!(removal.kept.is_empty(), "{removal:?}");
    assert!(keep.exists(), "an existing folder is never removed");
    assert!(keep.join("Update.exe").exists());
}

#[test]
fn a_non_leftover_existing_root_still_refuses() {
    let temp = tempfile::tempdir().expect("temp folder");
    std::fs::write(temp.path().join("other.txt"), b"x").expect("other");
    let chain = chain_under(temp.path(), &[]);
    assert!(matches!(Pin::fresh(&chain), Err(PinError::RootExists)));
}
