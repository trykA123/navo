use std::path::{Path, PathBuf};

use super::policy::FolderPolicy;
use super::*;

/// Every guarded folder sits under its own parent, so a parent holds only that folder.
fn policy(root: &Path) -> FolderPolicy {
    FolderPolicy {
        windows: Some(root.join("sys/Windows")),
        program_files: Some(root.join("pf/Program Files")),
        program_files_x86: Some(root.join("pf86/Program Files (x86)")),
        program_data: Some(root.join("pd/ProgramData")),
        profile: root.join("home/admin"),
        profiles: None,
        exe_folder: root.join("home/admin/Downloads/tools"),
        scratch_root: root.join("Local/navo-bench/run-1"),
    }
}

fn guarded(policy: &FolderPolicy) -> Vec<(PathBuf, SourceError)> {
    let some = |folder: &Option<PathBuf>| folder.clone().unwrap();
    vec![
        (some(&policy.windows), SourceError::WindowsFolder),
        (some(&policy.program_files), SourceError::ProgramFiles),
        (
            some(&policy.program_files_x86),
            SourceError::ProgramFilesX86,
        ),
        (some(&policy.program_data), SourceError::ProgramData),
        (policy.exe_folder.clone(), SourceError::ExeFolder),
        (policy.scratch_root.clone(), SourceError::ScratchRoot),
    ]
}

#[test]
fn a_folder_inside_a_guarded_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    for (folder, error) in guarded(&policy) {
        assert_eq!(policy.check(&folder), Err(error));
        assert_eq!(policy.check(&folder.join("child")), Err(error));
        assert_eq!(policy.check(&folder.join("child/deeper")), Err(error));
    }
}

#[test]
fn a_folder_containing_a_guarded_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    for (folder, error) in guarded(&policy) {
        let parent = folder.parent().unwrap();
        assert_eq!(policy.check(parent), Err(error));
    }
    let grandparent = policy.scratch_root.parent().unwrap().parent().unwrap();
    assert_eq!(policy.check(grandparent), Err(SourceError::ScratchRoot));
}

#[test]
fn neighbours_and_name_prefixes_of_guarded_folders_stay_allowed() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    for (folder, _) in guarded(&policy) {
        let mut sibling = folder.clone().into_os_string();
        sibling.push("2");
        assert_eq!(policy.check(Path::new(&sibling)), Ok(()));
        assert_eq!(policy.check(&folder.with_file_name("neighbour")), Ok(()));
    }
}

#[test]
fn the_profile_root_and_its_holders_are_refused_but_its_children_are_not() {
    let root = tempfile::tempdir().unwrap();
    let policy = policy(root.path());
    let home = policy.profile.parent().unwrap();
    assert_eq!(policy.check(home), Err(SourceError::UserProfileRoot));
    assert_eq!(
        policy.check(&policy.profile),
        Err(SourceError::UserProfileRoot)
    );
    assert_eq!(policy.check(&policy.profile.join("Documents")), Ok(()));
    assert_eq!(policy.check(&policy.profile.join("Documents/a")), Ok(()));
}

#[test]
fn the_downloads_folder_holding_the_exe_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = policy(root.path());
    policy.exe_folder = policy.profile.join("Downloads");
    assert_eq!(
        policy.check(&policy.profile.join("Downloads")),
        Err(SourceError::ExeFolder)
    );
    assert_eq!(policy.check(&policy.profile.join("Documents")), Ok(()));
}

#[test]
fn the_public_constructor_refuses_the_scratch_root_both_ways() {
    let outer = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir_in(outer.path()).unwrap();
    let inner = tempfile::tempdir_in(scratch.path()).unwrap();
    for path in [outer.path(), scratch.path(), inner.path()] {
        assert_eq!(
            ReadOnlySource::open(path, scratch.path()).unwrap_err(),
            SourceError::ScratchRoot
        );
    }
    let elsewhere = tempfile::tempdir().unwrap();
    assert!(ReadOnlySource::open(elsewhere.path(), scratch.path()).is_ok());
}

#[test]
fn the_public_constructor_refuses_the_exe_folder_both_ways() {
    let scratch = tempfile::tempdir().unwrap();
    let exe = std::fs::canonicalize(std::env::current_exe().unwrap()).unwrap();
    let folder = exe.parent().unwrap();
    for path in [folder, folder.parent().unwrap()] {
        assert_eq!(
            ReadOnlySource::open(path, scratch.path()).unwrap_err(),
            SourceError::ExeFolder
        );
    }
}

#[test]
fn a_missing_scratch_root_fails_closed() {
    let elsewhere = tempfile::tempdir().unwrap();
    let missing = elsewhere.path().join("missing");
    assert_eq!(
        ReadOnlySource::open(elsewhere.path(), &missing).unwrap_err(),
        SourceError::FolderLookup
    );
}
