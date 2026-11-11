// Windows tests for the chain pin: holds, rechecks, and cautious cleanup.

#![cfg(windows)]

use std::path::Path;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SHARING_VIOLATION, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::{
    Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    },
    DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, OPEN_EXISTING,
};

use super::{Pin, PinError, ancestors_unchanged};
use crate::payload::platform::security;
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
fn every_fresh_folder_has_a_protected_dacl_with_three_explicit_aces() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    let chain = chain_under(temp.path(), &["tools", "root"]);
    let _pin = Pin::fresh(&chain).expect("fresh pin");
    let full = security::user_sid()?;
    // SDDL writes the built-in Administrator (RID 500, as on CI runners) as the alias LA.
    let user = if full.ends_with("-500") { "LA".to_owned() } else { full };
    for name in ["tools", "root"] {
        let sddl = dacl_sddl(&temp.path().join(name))?;
        assert!(sddl.starts_with("D:P"), "{sddl}");
        assert_eq!(sddl.matches("(A;OICI;FA;;;").count(), 3, "{sddl}");
        for group in ["AU", "BU", "WD"] {
            assert!(!sddl.contains(&format!(";;;{group})")), "{sddl}");
        }
        assert!(sddl.contains(&format!(";;;{user})")), "{sddl}");
        assert!(sddl.contains(";;;SY)"), "{sddl}");
        assert!(sddl.contains(";;;BA)"), "{sddl}");
    }
    Ok(())
}

/// Reads a folder's DACL back as SDDL text.
fn dacl_sddl(path: &Path) -> std::io::Result<String> {
    let mut descriptor = std::ptr::null_mut();
    let mut acl = std::ptr::null_mut();
    // SAFETY: the wide path is NUL-terminated and every optional output is null or writable.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide_z(path).as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut acl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status.cast_signed()));
    }
    let _allocation = security::Local(descriptor);
    let mut text = std::ptr::null_mut();
    let mut length = 0;
    // SAFETY: the descriptor is live and the text and length outputs are writable.
    if unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            &mut length,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let _text = security::Local(text.cast());
    let length = usize::try_from(length).map_err(std::io::Error::other)?;
    // SAFETY: a successful conversion returns a live string of `length` UTF-16 units.
    Ok(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(text, length)
    }))
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
