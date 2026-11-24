// Windows tests for the chain probe: NTFS name comparison, protected targets, mandatory folders.

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_SHARING_VIOLATION};
use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

use super::{
    OrdinalNames, add_base, add_required_from, protected_from, resolve_held, resolve_path,
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
