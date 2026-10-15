use rusty_core::Location;

use super::*;

fn volume(text: &str) -> String {
    let location = Location::new(text).expect("test locations are valid");
    volume_of(&location).as_str().to_owned()
}

#[test]
fn ssd_limit_is_eight() {
    assert_eq!(VolumeKind::Ssd.limit(), 8);
}

#[test]
fn hdd_limit_is_one() {
    assert_eq!(VolumeKind::Hdd.limit(), 1);
}

#[test]
fn network_limit_is_four() {
    assert_eq!(VolumeKind::Network.limit(), 4);
}

#[test]
fn removable_limit_is_two() {
    assert_eq!(VolumeKind::Removable.limit(), 2);
}

#[test]
fn unknown_limit_is_one() {
    assert_eq!(VolumeKind::Unknown.limit(), 1);
}

#[test]
fn drive_path_gives_its_drive() {
    assert_eq!(volume(r"C:\a"), "C:");
}

#[test]
fn bare_lower_case_drive_gives_upper_case_drive() {
    assert_eq!(volume("c:"), "C:");
}

#[test]
fn unc_path_gives_its_share_in_lower_case() {
    assert_eq!(volume(r"\\Srv\Share\x"), r"\\srv\share");
}

#[test]
fn verbatim_drive_path_gives_its_drive() {
    assert_eq!(volume(r"\\?\D:\x"), "D:");
}

#[test]
fn verbatim_unc_path_gives_its_share() {
    assert_eq!(volume(r"\\?\UNC\srv\share\x"), r"\\srv\share");
}

#[test]
fn unix_path_gives_root() {
    assert_eq!(volume("/home/admin"), "/");
}

#[test]
fn other_provider_gives_its_root() {
    assert_eq!(volume("fake://a/b"), "fake://");
}

#[test]
fn same_share_in_different_case_gives_same_volume() {
    assert_eq!(volume(r"\\SRV\share\a"), volume(r"\\srv\SHARE\b"));
}
