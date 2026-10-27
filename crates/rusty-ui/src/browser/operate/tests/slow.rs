use rusty_core::Location;
use rusty_fs::DriveClass;

use crate::browser::operate::queue::{is_slow, slow_class};

#[test]
fn hard_disks_and_removable_drives_are_slow_everywhere() {
    assert!(slow_class(DriveClass::Hdd));
    assert!(slow_class(DriveClass::Removable));
    assert!(!slow_class(DriveClass::Ssd));
    assert!(!slow_class(DriveClass::Network));
}

#[test]
fn an_unknown_drive_is_slow_only_on_windows() {
    assert_eq!(slow_class(DriveClass::Unknown), cfg!(windows));
}

#[cfg(not(windows))]
#[test]
fn a_local_folder_off_windows_does_not_serialise_jobs() {
    let here = Location::new(std::env::temp_dir().to_string_lossy()).expect("temp dir location");
    assert!(!is_slow(&here));
}
