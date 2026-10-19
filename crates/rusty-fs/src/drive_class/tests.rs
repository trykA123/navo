use std::cell::Cell;

use super::*;
use crate::volume_of;

#[test]
fn remote_is_network_even_when_seek_penalty_fails() {
    assert_eq!(classify(true, None), DriveClass::Network);
    assert_eq!(classify(true, Some(true)), DriveClass::Network);
    assert_eq!(classify(true, Some(false)), DriveClass::Network);
}

#[test]
fn seek_penalty_is_hdd() {
    assert_eq!(classify(false, Some(true)), DriveClass::Hdd);
}

#[test]
fn no_seek_penalty_is_ssd() {
    assert_eq!(classify(false, Some(false)), DriveClass::Ssd);
}

#[test]
fn failed_seek_penalty_is_unknown() {
    assert_eq!(classify(false, None), DriveClass::Unknown);
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
fn system_drive_is_known() {
    let root = std::env::var("SystemDrive").expect("Windows defines SystemDrive");
    let location = Location::new(format!("{root}\\")).expect("valid system drive");
    assert_ne!(drive_class(&location), DriveClass::Unknown);
}

#[cfg(windows)]
#[test]
fn unc_path_is_network() {
    for path in [r"\\server\share\folder", r"\\?\UNC\server\share\folder"] {
        let location = Location::new(path).expect("valid UNC location");
        assert_eq!(drive_class(&location), DriveClass::Network);
    }
}
