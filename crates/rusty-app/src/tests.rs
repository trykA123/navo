use std::cell::Cell;
use std::ffi::OsString;
use std::process::ExitCode;

use rusty_store::StoreDirs;

#[test]
fn crash_startup_without_an_instance_guard_leaves_no_marker() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    let profile = crate::profile::Profile::User(Some(dirs.clone()));
    let (pending, running) = super::prepare_crash(&profile, None);
    assert!(pending.is_none());
    assert!(running.is_none());
    assert!(!dirs.data.join("running").exists());
}

#[test]
fn crash_startup_without_an_instance_guard_ignores_the_previous_marker() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs::in_profile(root.path());
    let _previous = crate::crash::Running::start(&dirs.data).unwrap();
    let marker = dirs.data.join("running");
    let before = std::fs::read(&marker).unwrap();
    let profile = crate::profile::Profile::User(Some(dirs));
    let (pending, running) = super::prepare_crash(&profile, None);
    assert!(pending.is_none());
    assert!(running.is_none());
    assert_eq!(std::fs::read(marker).unwrap(), before);
}

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
    let missing = root.path().join("missing");
    let status = super::execute([OsString::from("--left"), missing.into_os_string()], || {
        panic!("a refused pane detected user folders")
    });
    assert_eq!(status, ExitCode::from(2));
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
