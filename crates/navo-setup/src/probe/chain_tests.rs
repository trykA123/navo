// Windows tests for the chain probe: NTFS name comparison, protected targets, mandatory folders.

use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::Command;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_SHARING_VIOLATION};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;

use super::{
    OrdinalNames, absent_after, add_base, add_required_from, protected_from, resolve_held,
    resolve_path,
};
use crate::placement::{FileId, NameEq};

/// Plants a junction at `link` that points at `target`; `mklink /J` needs no elevation.
pub(crate) fn junction(link: &Path, target: &Path) {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()
        .expect("cmd runs");
    assert!(status.success(), "mklink /J plants the junction unelevated");
}

/// The identity of the last existing folder of `path`, without following a final link.
fn own_id(path: &Path) -> FileId {
    resolve_path(path)
        .expect("chain resolves")
        .existing
        .last()
        .expect("path exists")
        .id
}

/// A protected path that is a link (junction or mount point) anchors at what it points to.
#[test]
fn a_protected_link_anchors_at_its_target() {
    let root = tempfile::tempdir().expect("temp folder");
    let real = root.path().join("real");
    let link = root.path().join("link");
    std::fs::create_dir(&real).expect("junction target");
    junction(&link, &real);
    let target = own_id(&real);
    assert_ne!(own_id(&link), target, "the junction has its own id");

    let protected = protected_from(&link).expect("protected link resolves");
    assert_eq!(protected.anchor, target);
    assert!(protected.tail.is_empty());
    let below = protected_from(&link.join("missing")).expect("missing tail resolves");
    assert_eq!(below.anchor, target);
    assert_eq!(below.tail, [OsString::from("missing")]);
}

/// A protected link whose target is gone refuses; it never falls back to its parent folder.
#[test]
fn a_dangling_protected_link_refuses() {
    let root = tempfile::tempdir().expect("temp folder");
    let real = root.path().join("real");
    let link = root.path().join("link");
    std::fs::create_dir(&real).expect("junction target");
    junction(&link, &real);
    std::fs::remove_dir(&real).expect("the target goes away");
    assert!(protected_from(&link).is_err(), "the link itself refuses");
    assert!(
        protected_from(&link.join("missing")).is_err(),
        "a name below the link refuses"
    );
    assert!(
        protected_from(&root.path().join("absent")).is_ok(),
        "a truly absent name still anchors at its parent"
    );
}

/// A held chain keeps its ancestors in place: renaming one fails with a sharing violation.
#[test]
fn a_held_ancestor_cannot_be_renamed() {
    let root = tempfile::tempdir().expect("temp folder");
    let parent = root.path().join("parent");
    std::fs::create_dir(&parent).expect("parent folder");
    let held = resolve_held(&parent.join("Navo")).expect("chain resolves");
    assert_eq!(held.chain.missing, [OsString::from("Navo")]);
    let renamed = std::fs::rename(&parent, root.path().join("moved"));
    let sharing_violation = i32::try_from(ERROR_SHARING_VIOLATION).expect("error code fits i32");
    assert_eq!(
        renamed.map_err(|error| error.raw_os_error()),
        Err(Some(sharing_violation))
    );
    drop(held);
    std::fs::rename(&parent, root.path().join("moved")).expect("free once the chain drops");
}

/// The hold covers every ancestor, not only the nearest one.
///
/// A folder whose subtree has an open handle refuses a rename with access denied, so only a
/// sharing violation proves the grandparent itself is held.
#[test]
fn a_held_older_ancestor_cannot_be_renamed() {
    let root = tempfile::tempdir().expect("temp folder");
    let older = root.path().join("older");
    let parent = older.join("parent");
    std::fs::create_dir_all(&parent).expect("nested folders");
    let held = resolve_held(&parent.join("Navo")).expect("chain resolves");
    let renamed = std::fs::rename(&older, root.path().join("moved"));
    let sharing_violation = i32::try_from(ERROR_SHARING_VIOLATION).expect("error code fits i32");
    assert_eq!(
        renamed.map_err(|error| error.raw_os_error()),
        Err(Some(sharing_violation))
    );
    drop(held);
    std::fs::rename(&older, root.path().join("moved")).expect("free once the chain drops");
}

/// An ancestor that denies listing but allows traverse still resolves and is held.
#[test]
fn an_ancestor_that_denies_listing_still_resolves() {
    let root = tempfile::tempdir().expect("temp folder");
    let locked = root.path().join("locked");
    std::fs::create_dir_all(locked.join("child")).expect("nested folders");
    let icacls = |args: &[&str]| {
        let status = Command::new("icacls").arg(&locked).args(args).status();
        assert!(status.expect("icacls runs").success(), "icacls {args:?}");
    };
    icacls(&["/deny", "*S-1-1-0:(RD)"]);
    let held = resolve_held(&locked.join("child").join("Navo")).map(|held| held.chain);
    icacls(&["/remove:d", "*S-1-1-0"]);
    let chain = held.expect("traverse and attribute rights are enough");
    assert_eq!(chain.missing, [OsString::from("Navo")]);
}

/// A held empty folder can still take a mount point in place; the recheck sees it and refuses.
#[test]
fn a_held_folder_that_becomes_a_link_fails_the_recheck() {
    let root = tempfile::tempdir().expect("temp folder");
    let parent = root.path().join("parent");
    let elsewhere = root.path().join("elsewhere");
    std::fs::create_dir(&parent).expect("parent folder");
    std::fs::create_dir(&elsewhere).expect("link target");
    let held = resolve_held(&parent.join("Navo")).expect("chain resolves");
    held.recheck().expect("control: nothing changed yet");
    mount_point_in_place(&parent, &elsewhere)
        .expect("the hold's share mode does not stop an attribute-write open from linking");
    assert!(
        held.recheck().is_err(),
        "a held folder that became a link refuses"
    );
}

/// Only a not-found no-follow reopen counts as absent; a link that opens or any other error refuses.
#[test]
fn only_a_not_found_reopen_counts_as_absent() {
    assert!(absent_after::<()>(Err(io::ErrorKind::NotFound.into())).is_ok());
    assert!(absent_after(Ok(())).is_err());
    let denied = absent_after::<()>(Err(io::ErrorKind::PermissionDenied.into()));
    assert_eq!(
        denied.map_err(|error| error.kind()),
        Err(io::ErrorKind::PermissionDenied)
    );
}

/// Sets a mount point on an existing empty folder through an open with attribute-write access only.
pub(crate) fn mount_point_in_place(folder: &Path, target: &Path) -> io::Result<()> {
    /// `FSCTL_SET_REPARSE_POINT`, not exported by the enabled `windows-sys` features.
    const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
    let file = OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(folder)?;
    let buffer = mount_point_buffer(target);
    let size = u32::try_from(buffer.len()).expect("buffer size fits u32");
    let mut returned = 0u32;
    // SAFETY: the handle is live, the input buffer is initialized for `size` bytes, no output
    // buffer is passed and `returned` is writable.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            size,
            std::ptr::null_mut(),
            0,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A mount-point `REPARSE_DATA_BUFFER` for `target`, with an empty print name.
fn mount_point_buffer(target: &Path) -> Vec<u8> {
    let substitute: Vec<u16> = OsStr::new(r"\??\")
        .encode_wide()
        .chain(target.as_os_str().encode_wide())
        .collect();
    let substitute_bytes = u16::try_from(substitute.len() * 2).expect("name fits u16");
    // The substitute name, its NUL, then an empty print name and its NUL.
    let names: Vec<u16> = substitute.into_iter().chain([0, 0]).collect();
    let data_length = u16::try_from(8 + names.len() * 2).expect("buffer fits u16");
    let mut buffer = Vec::new();
    buffer.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    for field in [data_length, 0, 0, substitute_bytes, substitute_bytes + 2, 0] {
        buffer.extend(field.to_le_bytes());
    }
    for unit in names {
        buffer.extend(unit.to_le_bytes());
    }
    buffer
}

/// A mandatory known folder without a path refuses; only an optional one is skipped.
#[test]
fn a_mandatory_known_folder_without_a_path_refuses() {
    let mut out = Vec::new();
    assert!(add_required_from(&mut out, Ok(None), &[]).is_err());
    assert!(add_base(&mut out, None, &[]).is_ok());
    assert!(out.is_empty());
}

/// Creates the first name, then asks NTFS whether the second is the same name.
#[cfg(windows)]
#[test]
fn names_agree_with_ntfs() {
    let pairs = [
        ("navo", "NAVO"),
        ("ä", "Ä"),
        ("ß", "ẞ"),
        ("ı", "I"),
        ("i", "İ"),
        ("ǅ", "Ǆ"),
        ("σ", "Σ"),
        ("ς", "Σ"),
        ("K", "\u{212A}"),
        ("Å", "\u{212B}"),
    ];
    let names = OrdinalNames;
    for (first, second) in pairs {
        let root = tempfile::tempdir().expect("temp folder");
        let first_path = root.path().join(first);
        std::fs::create_dir(&first_path).expect("first name");
        let second_path = root.path().join(second);
        let second_wide = wide_z(&second_path);
        // SAFETY: the path is NUL-terminated and its temporary folder stays alive for the call.
        let created = unsafe { CreateDirectoryW(second_wide.as_ptr(), std::ptr::null()) };
        let error = std::io::Error::last_os_error();
        if created == 0
            && error.raw_os_error()
                != Some(i32::try_from(ERROR_ALREADY_EXISTS).expect("error code fits i32"))
        {
            panic!("CreateDirectoryW failed for {second:?}: {error}");
        }
        if created == 0 {
            assert!(
                names.same(OsStr::new(first), OsStr::new(second)),
                "NTFS treats {first:?} and {second:?} as one name",
            );
        }
    }
}

/// A NUL-terminated UTF-16 path for `CreateDirectoryW`.
fn wide_z(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
