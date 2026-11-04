use std::fs;
use std::path::Path;

use super::{GuardError, PathRole, ScratchRoot};

const ROLE: PathRole = PathRole::ScratchRoot;

#[allow(
    clippy::disallowed_methods,
    reason = "guard tests plant symlinks in isolated scratch directories"
)]
fn plant_symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[test]
fn medium_symlink_out_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let link = scratch.path().join("link-out");
    plant_symlink(outside.path(), &link);
    assert_eq!(
        scratch.check(&link.join("new.txt")).unwrap_err(),
        GuardError::ReparsePoint(ROLE)
    );
}

#[test]
fn medium_symlink_inside_root_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let target = tempfile::tempdir_in(scratch.path()).unwrap();
    let link = scratch.path().join("link-inside");
    plant_symlink(target.path(), &link);
    assert_eq!(
        scratch.check(&link.join("new.txt")).unwrap_err(),
        GuardError::ReparsePoint(ROLE)
    );
}

#[test]
fn medium_symlink_parent_is_refused_before_root_creation() {
    let parent = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = parent.path().join("parent-link");
    plant_symlink(outside.path(), &link);
    assert_eq!(
        ScratchRoot::create(&link, None).unwrap_err(),
        GuardError::ReparsePoint(ROLE)
    );
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn medium_owner_symlink_is_refused_with_owner_role() {
    let parent = tempfile::tempdir().unwrap();
    let owner = tempfile::tempdir().unwrap();
    let link = parent.path().join("owner-link");
    plant_symlink(owner.path(), &link);
    assert_eq!(
        ScratchRoot::create(parent.path(), Some(&link)).unwrap_err(),
        GuardError::ReparsePoint(PathRole::OwnerFolder)
    );
}

#[test]
fn medium_case_different_spelling_is_not_a_linux_alias() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let name = scratch
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_uppercase();
    let alias = scratch.path().with_file_name(name);
    assert_eq!(
        scratch.check(&alias.join("new.txt")).unwrap_err(),
        GuardError::OutsideRoot(ROLE)
    );
}

#[test]
fn medium_non_utf8_destination_is_preserved() {
    use std::os::unix::ffi::OsStringExt;
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let destination = scratch
        .path()
        .join(std::ffi::OsString::from_vec(vec![0xff, b'x']));
    assert_eq!(
        scratch.check(&destination).unwrap().resolved.path,
        destination
    );
}
