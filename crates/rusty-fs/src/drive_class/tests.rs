use std::cell::Cell;

use super::*;
use crate::volume_of;

#[test]
fn remote_is_network_for_all_query_results() {
    for seek_penalty in [None, Some(true), Some(false)] {
        for trim_enabled in [None, Some(true), Some(false)] {
            assert_eq!(
                classify(true, seek_penalty, trim_enabled),
                DriveClass::Network
            );
        }
    }
}

#[test]
fn seek_penalty_is_hdd() {
    for trim_enabled in [None, Some(true), Some(false)] {
        assert_eq!(classify(false, Some(true), trim_enabled), DriveClass::Hdd);
    }
}

#[test]
fn no_seek_penalty_is_ssd() {
    for trim_enabled in [None, Some(true), Some(false)] {
        assert_eq!(classify(false, Some(false), trim_enabled), DriveClass::Ssd);
    }
}

#[test]
fn failed_seek_penalty_with_trim_is_ssd() {
    assert_eq!(classify(false, None, Some(true)), DriveClass::Ssd);
}

#[test]
fn failed_seek_penalty_is_unknown() {
    for trim_enabled in [None, Some(false)] {
        assert_eq!(classify(false, None, trim_enabled), DriveClass::Unknown);
    }
}

#[test]
fn same_volume_uses_first_probe_result() {
    let cache = Mutex::new(HashMap::new());
    let probes = Cell::new(0);
    for path in [r"c:\first", r"C:\second", r"\\?\C:\third"] {
        let location = Location::new(path).expect("valid test location");
        let class = cached_class(&cache, volume_of(&location), |_| {
            probes.set(probes.get() + 1);
            if probes.get() == 1 {
                DriveClass::Ssd
            } else {
                DriveClass::Hdd
            }
        });
        assert_eq!(class, DriveClass::Ssd);
    }
    assert_eq!(probes.get(), 1);
}

#[test]
fn unc_share_uses_first_probe_result() {
    let cache = Mutex::new(HashMap::new());
    let probes = Cell::new(0);
    for path in [r"\\server\Share\first", r"\\?\UNC\SERVER\share\second"] {
        let location = Location::new(path).expect("valid test location");
        assert_eq!(
            cached_class(&cache, volume_of(&location), |_| {
                probes.set(probes.get() + 1);
                DriveClass::Network
            }),
            DriveClass::Network
        );
    }
    assert_eq!(probes.get(), 1);
}

#[test]
fn failed_probe_is_cached() {
    let cache = Mutex::new(HashMap::new());
    let location = Location::new(r"C:\first").expect("valid test location");
    assert_eq!(
        cached_class(&cache, volume_of(&location), |_| DriveClass::Unknown),
        DriveClass::Unknown
    );
    assert_eq!(
        cached_class(&cache, volume_of(&location), |_| DriveClass::Ssd),
        DriveClass::Unknown
    );
}

#[cfg(not(windows))]
#[test]
fn other_platforms_are_unknown() {
    for path in ["/", r"C:\", r"\\server\share\folder"] {
        let location = Location::new(path).expect("valid test location");
        assert_eq!(drive_class(&location), DriveClass::Unknown);
    }
}

#[cfg(windows)]
#[test]
#[expect(
    clippy::print_stderr,
    reason = "reports Windows storage query errors in CI"
)]
fn system_drive_is_local() {
    let root = std::env::var("SystemDrive").expect("Windows defines SystemDrive");
    let location = Location::new(format!("{root}\\")).expect("valid system drive");
    let class = drive_class(&location);
    if class == DriveClass::Unknown {
        let (seek_error, trim_error) = windows::query_errors(&root);
        eprintln!(
            "system drive {root}: seek penalty GetLastError={seek_error}, trim GetLastError={trim_error}"
        );
    }
    assert_ne!(class, DriveClass::Network);
}

#[cfg(windows)]
#[test]
fn unc_path_is_network() {
    for path in [r"\\server\share\folder", r"\\?\UNC\server\share\folder"] {
        let location = Location::new(path).expect("valid UNC location");
        assert_eq!(drive_class(&location), DriveClass::Network);
    }
}
