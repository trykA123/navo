use std::cell::Cell;
use std::ffi::OsString;
use std::process::ExitCode;

use rusty_store::StoreDirs;

#[test]
fn a_refused_command_never_detects_or_writes_user_folders() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    let detected = Cell::new(false);
    let status = super::execute([OsString::from("--metrics")], || {
        detected.set(true);
        Some(dirs.clone())
    });
    assert_eq!(status, ExitCode::from(2));
    assert!(!detected.get(), "a refused command detected user folders");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn a_missing_pane_folder_fails_before_opening_a_window() {
    let root = tempfile::tempdir().unwrap();
    let report = root.path().join("metrics.json");
    let missing = root.path().join("missing");
    let args = [
        OsString::from("--metrics"),
        report.as_os_str().to_owned(),
        OsString::from("--left"),
        missing.as_os_str().to_owned(),
    ];
    let status = super::execute(args, || panic!("metrics detected user folders"));
    assert_eq!(status, ExitCode::from(2));
    assert!(!report.exists());
}
