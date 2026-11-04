use std::fs;
use std::path::{Path, PathBuf};

use super::{GuardError, MARKER_FILE, PathRole, ScratchRoot, forbidden_roots, placement};

const ROLE: PathRole = PathRole::ScratchRoot;

#[test]
fn medium_new_root_has_unique_name_and_matching_marker() {
    let parent = tempfile::tempdir().unwrap();
    let first = ScratchRoot::create(parent.path(), None).unwrap();
    let second = ScratchRoot::create(parent.path(), None).unwrap();
    assert_ne!(first.path(), second.path());
    assert_ne!(first.token(), second.token());
    assert!(
        first
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("run-")
    );
    assert_eq!(
        fs::read(first.path().join(MARKER_FILE)).unwrap(),
        first.token()
    );
    assert!(first.check(&first.path().join(MARKER_FILE)).is_ok());
}

#[test]
fn medium_missing_descendants_are_checked_against_existing_ancestor() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let destination = scratch.path().join("new/child/file.txt");
    let checked = scratch.guard().check(&destination).unwrap();
    assert_eq!(checked.resolved.path, destination);
    assert!(!destination.exists());
}

#[test]
fn medium_outside_path_is_refused() {
    let fixture = crate::fixture::set().unwrap();
    let scratch = ScratchRoot::create(fixture.root(), None).unwrap();
    assert_eq!(
        scratch.check(&fixture.marker_file).unwrap_err(),
        GuardError::OutsideRoot(ROLE)
    );
}

#[test]
fn medium_sibling_with_matching_text_prefix_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let sibling = scratch.path().with_file_name(format!(
        "{}-other",
        scratch.path().file_name().unwrap().to_string_lossy()
    ));
    assert_eq!(
        scratch.check(&sibling).unwrap_err(),
        GuardError::OutsideRoot(ROLE)
    );
}

/// Appends `parts` as text: `join` on a Windows `\\?\` root drops `..` before the guard sees it.
fn raw_join(base: &Path, parts: &[&str]) -> PathBuf {
    let mut text = base.as_os_str().to_owned();
    for part in parts {
        text.push(std::path::MAIN_SEPARATOR_STR);
        text.push(part);
    }
    PathBuf::from(text)
}

#[test]
fn medium_parent_component_escape_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    assert_eq!(
        scratch
            .check(&raw_join(scratch.path(), &["..", "outside"]))
            .unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_parent_component_staying_inside_is_also_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    assert_eq!(
        scratch
            .check(&raw_join(scratch.path(), &["new", "..", "other"]))
            .unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_relative_path_is_refused() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    assert_eq!(
        scratch.check(Path::new("relative.txt")).unwrap_err(),
        GuardError::InvalidPath(ROLE)
    );
}

#[test]
fn medium_root_inside_owner_is_refused_before_creation() {
    let parent = tempfile::tempdir().unwrap();
    let result = ScratchRoot::create(parent.path(), Some(parent.path()));
    assert_eq!(
        result.unwrap_err(),
        GuardError::OwnerOverlap(PathRole::OwnerFolder)
    );
    assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn medium_owner_inside_root_is_refused_before_creation() {
    let parent = tempfile::tempdir().unwrap();
    let owner = parent.path().join("run-fixed/owner");
    let result = placement::create_test_root(parent.path(), Some(&owner), "run-fixed", &[]);
    assert_eq!(
        result.unwrap_err(),
        GuardError::OwnerOverlap(PathRole::OwnerFolder)
    );
    assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn medium_disjoint_owner_is_allowed() {
    let parent = tempfile::tempdir().unwrap();
    let owner = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), Some(owner.path())).unwrap();
    assert!(scratch.path().is_dir());
    assert_eq!(fs::read_dir(owner.path()).unwrap().count(), 0);
}

fn assert_forbidden_parent(label: &str) {
    let parent = tempfile::Builder::new().prefix(label).tempdir().unwrap();
    let forbidden = forbidden_roots(Some(&[parent.path().to_path_buf()])).unwrap();
    let result = placement::create_test_root(parent.path(), None, "run-fixed", &forbidden);
    assert_eq!(result.unwrap_err(), GuardError::ForbiddenParent(ROLE));
    assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn medium_drive_root_parent_is_refused() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().ancestors().last().unwrap();
    let result = placement::create_test_root(root, None, "run-fixed", &[]);
    assert_eq!(result.unwrap_err(), GuardError::ForbiddenParent(ROLE));
}

#[test]
fn medium_windows_folder_parent_is_refused() {
    assert_forbidden_parent("windows-");
}

#[test]
fn medium_program_files_parent_is_refused() {
    assert_forbidden_parent("program-files-");
}

#[test]
fn medium_program_files_x86_parent_is_refused() {
    assert_forbidden_parent("program-files-x86-");
}

#[test]
fn medium_profile_root_parent_is_refused() {
    assert_forbidden_parent("profile-");
}

#[test]
fn medium_profile_descendant_parent_is_allowed() {
    let profile = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir_in(profile.path()).unwrap();
    let forbidden = forbidden_roots(Some(&[profile.path().to_path_buf()])).unwrap();
    let scratch =
        placement::create_test_root(parent.path(), None, "run-fixed", &forbidden).unwrap();
    assert!(scratch.check(&scratch.path().join("generated.txt")).is_ok());
}

#[test]
fn medium_missing_parent_fails_without_creating_directories() {
    let parent = tempfile::tempdir().unwrap();
    let missing = parent.path().join("missing");
    assert_eq!(
        ScratchRoot::create(&missing, None).unwrap_err(),
        GuardError::Io(ROLE)
    );
    assert!(!missing.exists());
}

#[test]
fn medium_existing_run_is_never_reused_or_overwritten() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = placement::create_test_root(parent.path(), None, "run-fixed", &[]).unwrap();
    let marker_before = fs::read(scratch.path().join(MARKER_FILE)).unwrap();
    assert_eq!(
        placement::create_test_root(parent.path(), None, "run-fixed", &[]).unwrap_err(),
        GuardError::Io(ROLE)
    );
    assert_eq!(
        fs::read(scratch.path().join(MARKER_FILE)).unwrap(),
        marker_before
    );
}

#[test]
fn medium_dropping_root_retains_marked_directory_for_cleanup() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let root = scratch.path().to_path_buf();
    let token = *scratch.token();
    drop(scratch);
    assert_eq!(fs::read(root.join(MARKER_FILE)).unwrap(), token);
}

#[test]
fn medium_errors_and_debug_do_not_disclose_paths_or_token() {
    let parent = tempfile::Builder::new()
        .prefix("private-planted-folder-")
        .tempdir()
        .unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let error = scratch
        .check(&parent.path().join("private-name.txt"))
        .unwrap_err();
    assert_eq!(error.to_string(), "scratch-root: outside-root");
    assert_eq!(format!("{scratch:?}"), "ScratchRoot { scratch-root }");
    assert_eq!(format!("{:?}", scratch.guard()), "Guard { scratch-root }");
    assert_eq!(
        format!("{:?}", scratch.check(scratch.path()).unwrap()),
        "CheckedPath { scratch-root }"
    );
}
