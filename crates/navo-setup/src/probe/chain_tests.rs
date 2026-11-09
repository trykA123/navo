// Windows test for the chain probe's NTFS name comparison.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

use super::OrdinalNames;
use crate::placement::NameEq;

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
