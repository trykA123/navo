//! Tests for WSL distro entries in the drive list (NV11a): a fake distro list
//! stands in for the Windows registry, so these run on every platform.

use super::{Drive, DriveKind, RawDrives, assemble};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|&name| name.to_owned()).collect()
}

fn wsl_roots(drives: &[Drive]) -> Vec<&str> {
    drives
        .iter()
        .filter(|drive| drive.root.starts_with(r"\\wsl.localhost\"))
        .map(|drive| drive.root.as_str())
        .collect()
}

#[test]
fn each_installed_distro_is_listed_after_the_letters() {
    let raw = RawDrives {
        mask: 1 << 2,
        kinds: vec![('C', DriveKind::Fixed)],
        wsl: names(&["Ubuntu-24.04", "Debian"]),
        ..RawDrives::default()
    };
    let drives = assemble(&raw);
    assert_eq!(drives.len(), 3);
    assert_eq!(drives[0].letter, Some('C'));
    let ubuntu = &drives[1];
    assert_eq!(ubuntu.root, r"\\wsl.localhost\Ubuntu-24.04\");
    assert_eq!(ubuntu.letter, None);
    assert_eq!(ubuntu.kind, DriveKind::Network);
    assert_eq!(ubuntu.label.as_deref(), Some("Ubuntu-24.04"));
    assert_eq!(ubuntu.remote, None);
    assert!(ubuntu.connected);
    assert_eq!(drives[2].root, r"\\wsl.localhost\Debian\");
}

#[test]
fn no_wsl_means_no_extra_entries() {
    let raw = RawDrives {
        mask: 1 << 2,
        kinds: vec![('C', DriveKind::Fixed)],
        ..RawDrives::default()
    };
    assert!(wsl_roots(&assemble(&raw)).is_empty());
}

#[test]
fn blank_duplicate_and_path_breaking_names_are_skipped() {
    let raw = RawDrives {
        wsl: names(&[
            "",
            "  ",
            "Ubuntu",
            "ubuntu",
            r"bad\name",
            "bad/name",
            "Alpine",
        ]),
        ..RawDrives::default()
    };
    assert_eq!(
        wsl_roots(&assemble(&raw)),
        vec![r"\\wsl.localhost\Ubuntu\", r"\\wsl.localhost\Alpine\"]
    );
}
