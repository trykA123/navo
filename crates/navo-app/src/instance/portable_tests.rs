//! Portable runtime folder tests (NV9a): a portable copy keeps its instance lock and socket in
//! `<root>/data/run`, and every unsafe folder starts Navo without a lock instead of elsewhere.

use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::Path;

use super::{Acquired, acquire_portable};

/// Lists the file names directly inside `folder`, sorted; fails when the folder cannot be read.
fn names(folder: &Path) -> std::io::Result<Vec<String>> {
    let mut names = std::fs::read_dir(folder)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<Vec<String>>>()?;
    names.sort();
    Ok(names)
}

#[test]
fn a_portable_copy_keeps_its_lock_and_socket_in_data_run() {
    let root = tempfile::tempdir().unwrap();
    let run = root.path().join("data").join("run");
    let acquired = acquire_portable(root.path(), &run).unwrap();
    assert!(matches!(acquired, Some(Acquired::First(_))));
    assert_eq!(names(root.path()).unwrap(), ["data"]);
    let metadata = std::fs::symlink_metadata(&run).unwrap();
    assert!(metadata.is_dir());
    assert_eq!(metadata.mode() & 0o777, 0o700);
    let files = names(&run).unwrap();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files.iter().any(|name| name.ends_with(".lock")));
    assert!(files.iter().any(|name| name.ends_with(".sock")));
    assert!(matches!(
        acquire_portable(root.path(), &run).unwrap(),
        Some(Acquired::Second(_))
    ));
}

#[test]
fn a_copied_run_folder_with_open_permissions_is_made_private() {
    let root = tempfile::tempdir().unwrap();
    let run = root.path().join("data").join("run");
    std::fs::create_dir_all(&run).unwrap();
    std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o755)).unwrap();
    let acquired = acquire_portable(root.path(), &run).unwrap();
    assert!(matches!(acquired, Some(Acquired::First(_))));
    assert_eq!(std::fs::metadata(&run).unwrap().mode() & 0o777, 0o700);
}

#[test]
fn a_run_folder_that_is_a_symlink_starts_without_a_lock() {
    let root = tempfile::tempdir().unwrap();
    let elsewhere = root.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::create_dir(root.path().join("data")).unwrap();
    let run = root.path().join("data").join("run");
    symlink(&elsewhere, &run).unwrap();
    assert!(acquire_portable(root.path(), &run).unwrap().is_none());
    assert!(names(&elsewhere).unwrap().is_empty());
}

#[test]
fn a_run_folder_too_long_for_a_socket_starts_without_a_lock() {
    let root = tempfile::tempdir().unwrap();
    let run = root.path().join("x".repeat(90)).join("data").join("run");
    assert!(acquire_portable(root.path(), &run).unwrap().is_none());
    assert_eq!(names(root.path()).unwrap(), Vec::<String>::new());
}

#[test]
fn a_file_in_place_of_the_data_folder_starts_without_a_lock() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("data"), b"").unwrap();
    let run = root.path().join("data").join("run");
    assert!(acquire_portable(root.path(), &run).unwrap().is_none());
}
