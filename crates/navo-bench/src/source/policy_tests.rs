use super::policy::FolderPolicy;
use super::*;

fn policy() -> (tempfile::TempDir, FolderPolicy) {
    let root = tempfile::tempdir().unwrap();
    let policy = FolderPolicy {
        windows: Some(root.path().join("Windows")),
        program_files: Some(root.path().join("Program Files")),
        program_files_x86: Some(root.path().join("Program Files (x86)")),
        program_data: Some(root.path().join("ProgramData")),
        profile: root.path().join("Users/admin"),
        profiles: Some(root.path().join("Users")),
        exe_folder: root.path().join("Users/admin/Downloads/tools"),
        scratch_root: root.path().join("Local/navo-bench/run-1"),
    };
    (root, policy)
}

#[test]
fn drive_root_has_a_typed_refusal() {
    let (root, mut policy) = policy();
    policy.windows = None;
    let drive = root.path().ancestors().last().unwrap();
    assert_eq!(policy.check(drive), Err(SourceError::DriveRoot));
}

#[test]
fn system_drive_root_has_a_typed_refusal() {
    let (root, policy) = policy();
    let drive = root.path().ancestors().last().unwrap();
    assert_eq!(policy.check(drive), Err(SourceError::SystemDriveRoot));
}

#[test]
fn windows_folder_has_a_typed_refusal() {
    let (_root, policy) = policy();
    assert_eq!(
        policy.check(policy.windows.as_ref().unwrap()),
        Err(SourceError::WindowsFolder)
    );
}

#[test]
fn program_files_has_a_typed_refusal() {
    let (_root, policy) = policy();
    assert_eq!(
        policy.check(policy.program_files.as_ref().unwrap()),
        Err(SourceError::ProgramFiles)
    );
}

#[test]
fn program_files_x86_has_a_typed_refusal() {
    let (_root, policy) = policy();
    assert_eq!(
        policy.check(policy.program_files_x86.as_ref().unwrap()),
        Err(SourceError::ProgramFilesX86)
    );
}

#[test]
fn each_profile_root_is_refused_but_its_children_are_allowed() {
    let (_root, policy) = policy();
    assert_eq!(
        policy.check(&policy.profile),
        Err(SourceError::UserProfileRoot)
    );
    assert_eq!(policy.check(&policy.profile.join("Documents")), Ok(()));
    let other = policy.profiles.as_ref().unwrap().join("other");
    assert_eq!(policy.check(&other), Err(SourceError::UserProfileRoot));
    assert_eq!(policy.check(&other.join("Documents")), Ok(()));
}

#[test]
fn unc_server_roots_are_refused_before_filesystem_access() {
    for path in [
        r"\\server",
        r"\\server\",
        r"\\?\UNC\server",
        r"\\?\UNC\server\",
    ] {
        assert_eq!(
            crate::source::test_open(Path::new(path)).unwrap_err(),
            SourceError::UncServerRoot
        );
    }
}

#[cfg(windows)]
#[test]
fn known_folder_refusals_apply_through_the_public_constructor() {
    let scratch = tempfile::tempdir().unwrap();
    let policy = FolderPolicy::load(scratch.path()).unwrap();
    for (path, error) in [
        (policy.windows.as_ref().unwrap(), SourceError::WindowsFolder),
        (
            policy.program_files.as_ref().unwrap(),
            SourceError::ProgramFiles,
        ),
        (
            policy.program_files_x86.as_ref().unwrap(),
            SourceError::ProgramFilesX86,
        ),
        (
            policy.program_data.as_ref().unwrap(),
            SourceError::ProgramData,
        ),
        (&policy.profile, SourceError::UserProfileRoot),
    ] {
        assert_eq!(crate::source::test_open(path).unwrap_err(), error);
    }
    let system_root = policy.windows.as_ref().unwrap().ancestors().last().unwrap();
    assert_eq!(
        crate::source::test_open(system_root).unwrap_err(),
        SourceError::SystemDriveRoot
    );
}
