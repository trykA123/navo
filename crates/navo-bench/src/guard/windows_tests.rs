use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

use super::{GuardError, MARKER_FILE, PathRole, ScratchRoot};

const ROLE: PathRole = PathRole::ScratchRoot;

#[allow(
    clippy::disallowed_methods,
    reason = "guard test runs mklink to plant a junction in its temp tree"
)]
fn junction(target: &Path, link: &Path) {
    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "junction fixture creation must succeed"
    );
}

#[allow(
    clippy::disallowed_methods,
    reason = "guard test plants an escaping Windows symlink"
)]
fn symlink(target: &Path, link: &Path) {
    std::os::windows::fs::symlink_dir(target, link).unwrap();
}

fn short_path(path: &Path) -> PathBuf {
    let input: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut output = vec![0_u16; 32768];
    // SAFETY: input is NUL-terminated and output has the supplied writable capacity.
    let length = unsafe { GetShortPathNameW(input.as_ptr(), output.as_mut_ptr(), 32768) };
    assert!(
        length > 0 && length < 32768,
        "8.3 fixture lookup must succeed"
    );
    output.truncate(usize::try_from(length).unwrap());
    let short = PathBuf::from(OsString::from_wide(&output));
    assert!(
        short.as_os_str().encode_wide().any(|unit| unit == 126),
        "test volume must provide an actual 8.3 alias"
    );
    short
}

#[test]
fn medium_junction_out_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let link = scratch.path().join("junction-out");
    junction(outside.path(), &link);
    assert_eq!(
        scratch.check(&link.join("new.txt")).unwrap_err(),
        GuardError::ReparsePoint(ROLE)
    );
}

#[test]
fn medium_windows_symlink_out_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let link = scratch.path().join("symlink-out");
    symlink(outside.path(), &link);
    assert_eq!(
        scratch.check(&link.join("new.txt")).unwrap_err(),
        GuardError::ReparsePoint(ROLE)
    );
}

#[test]
fn medium_reparse_point_inside_root_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let target = tempfile::tempdir_in(scratch.path()).unwrap();
    let link = scratch.path().join("junction-inside");
    junction(target.path(), &link);
    assert_eq!(
        scratch.check(&link.join("new.txt")).unwrap_err(),
        GuardError::ReparsePoint(ROLE)
    );
}

#[test]
fn medium_case_different_spelling_resolves_to_same_root() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let name = scratch
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_uppercase();
    let alias = scratch.path().with_file_name(name).join(MARKER_FILE);
    assert!(scratch.check(&alias).is_ok());
}

#[test]
fn medium_eight_dot_three_alias_resolves_inside_root() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let alias = short_path(scratch.path());
    assert!(scratch.check(&alias.join("new.txt")).is_ok());
}

#[test]
fn medium_eight_dot_three_alias_outside_root_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let alias = short_path(outside.path());
    assert_eq!(
        scratch.check(&alias.join("new.txt")).unwrap_err(),
        GuardError::OutsideRoot(ROLE)
    );
}

#[test]
fn medium_extended_dos_prefix_is_canonicalized() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let canonical = fs::canonicalize(scratch.path()).unwrap();
    assert!(canonical.as_os_str().to_string_lossy().starts_with(r"\\?\"));
    assert!(scratch.check(&canonical.join("new.txt")).is_ok());
}

#[test]
fn medium_globalroot_device_path_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let path = Path::new(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy1\private");
    assert_eq!(
        scratch.check(path).unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_dos_device_path_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    assert_eq!(
        scratch.check(Path::new(r"\\.\C:\private")).unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_alternate_data_stream_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let stream = scratch.path().join("file.txt::$DATA");
    assert_eq!(
        scratch.check(&stream).unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_trailing_dot_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    assert_eq!(
        scratch.check(&scratch.path().join("file.")).unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_trailing_space_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    assert_eq!(
        scratch.check(&scratch.path().join("file ")).unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_reserved_dos_name_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    assert_eq!(
        scratch.check(&scratch.path().join("NUL.txt")).unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_root_handle_blocks_directory_rename() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    assert!(rename_attempt(scratch.path(), &parent.path().join("renamed")).is_err());
    let root = scratch.path().to_path_buf();
    drop(scratch);
    rename_attempt(&root, &parent.path().join("renamed")).unwrap();
}

#[allow(
    clippy::disallowed_methods,
    reason = "guard test verifies held Windows handles deny replacement"
)]
fn rename_attempt(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

/// A check holds attribute-only handles, which pin nothing; the act holds the parents instead
/// (`act_windows_tests::large_a_held_parent_cannot_be_swapped_during_a_create`).
#[test]
fn medium_a_checked_path_alone_does_not_pin_its_folders() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let child = tempfile::tempdir_in(scratch.path()).unwrap();
    let checked = scratch.check(&child.path().join("new.txt")).unwrap();
    rename_attempt(child.path(), &scratch.path().join("renamed")).unwrap();
    drop(checked);
}

#[test]
fn small_windows_comparison_uses_unicode_ordinal_case() {
    assert!(super::platform::equal(
        OsStr::new("Étage"),
        OsStr::new("éTAGE")
    ));
}
