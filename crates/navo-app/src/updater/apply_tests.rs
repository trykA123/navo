use std::cell::Cell;
use std::ffi::OsString;
use std::io;

use navo_update::{InstallKind, Version};

use super::{Apply, Reason, Staged, apply_decision, has_relative_value, unreadable_counts};
use crate::cli::{Command, Start};

fn open() -> Command {
    Command::Open(Start::default())
}

fn staged(current: &str, version: &str) -> Option<Staged<&'static str>> {
    Some(Staged {
        current: Version::parse(current).expect("current version"),
        version: Version::parse(version).expect("staged version"),
        asset: "package",
    })
}

fn newer() -> Option<Staged<&'static str>> {
    staged("0.0.1-alpha.20261030c", "0.0.1-alpha.20261102a")
}

fn alone() -> io::Result<bool> {
    Ok(false)
}

#[test]
fn apply_installed_newer_alone_window_applies_now() {
    let got = apply_decision(InstallKind::Installed, newer(), alone, &open());
    assert_eq!(got, Apply::Now("package"));
}

#[test]
fn apply_portable_waits_without_querying_processes() {
    let asked = Cell::new(false);
    let got = apply_decision(
        InstallKind::Portable,
        newer(),
        || {
            asked.set(true);
            Ok(false)
        },
        &open(),
    );
    assert_eq!(
        got,
        Apply::Wait(Reason::NotInstalled(InstallKind::Portable))
    );
    assert!(!asked.get());
}

#[test]
fn apply_unpackaged_waits() {
    let got = apply_decision(InstallKind::Unpackaged, newer(), alone, &open());
    assert_eq!(
        got,
        Apply::Wait(Reason::NotInstalled(InstallKind::Unpackaged))
    );
}

#[test]
fn apply_nothing_staged_waits() {
    let got = apply_decision::<&str>(InstallKind::Installed, None, alone, &open());
    assert_eq!(got, Apply::Wait(Reason::NothingStaged));
}

#[test]
fn apply_older_staged_waits() {
    let got = apply_decision(
        InstallKind::Installed,
        staged("0.0.1-alpha.20261102a", "0.0.1-alpha.20261030c"),
        alone,
        &open(),
    );
    assert_eq!(got, Apply::Wait(Reason::NotNewer));
}

#[test]
fn apply_equal_staged_waits() {
    let got = apply_decision(
        InstallKind::Installed,
        staged("0.1.0", "0.1.0"),
        alone,
        &open(),
    );
    assert_eq!(got, Apply::Wait(Reason::NotNewer));
}

#[test]
fn apply_another_process_waits() {
    let got = apply_decision(InstallKind::Installed, newer(), || Ok(true), &open());
    assert_eq!(got, Apply::Wait(Reason::OthersRunning));
}

#[test]
fn apply_failed_process_query_waits() {
    let got = apply_decision(
        InstallKind::Installed,
        newer(),
        || Err(io::ErrorKind::PermissionDenied.into()),
        &open(),
    );
    assert_eq!(got, Apply::Wait(Reason::QueryFailed));
}

#[test]
fn apply_non_window_command_waits_without_querying_processes() {
    let asked = Cell::new(false);
    let got = apply_decision(
        InstallKind::Installed,
        newer(),
        || {
            asked.set(true);
            Ok(false)
        },
        &Command::Version,
    );
    assert_eq!(got, Apply::Wait(Reason::NotWindow));
    assert!(!asked.get());
}

#[test]
fn apply_process_query_without_a_root_or_off_windows_reports_running() {
    // Off Windows the stub must report "running" so nothing ever applies there.
    #[cfg(not(windows))]
    assert!(matches!(super::others_running(None), Ok(true)));
    #[cfg(windows)]
    assert!(super::others_running(None).is_err());
}

#[test]
fn an_unreadable_image_blocks_when_its_name_looks_like_navo() {
    assert!(unreadable_counts("navo.exe", "navo.exe"));
    assert!(unreadable_counts("Navo-Index.exe", "navo.exe"));
    assert!(unreadable_counts("renamed.exe", "renamed.exe"));
    assert!(!unreadable_counts("csrss.exe", "navo.exe"));
}

#[test]
fn relative_values_wait_but_flags_and_absolute_paths_do_not() {
    let args = |items: &[&str]| items.iter().map(OsString::from).collect::<Vec<_>>();
    assert!(!has_relative_value(&args(&[])));
    assert!(!has_relative_value(&args(&["--no-retention"])));
    assert!(has_relative_value(&args(&["--profile", "profile"])));
    assert!(has_relative_value(&args(&["docs"])));
    #[cfg(unix)]
    assert!(!has_relative_value(&args(&["--left", "/data/docs"])));
    #[cfg(windows)]
    assert!(!has_relative_value(&args(&["--left", r"C:\data\docs"])));
}
