use super::{DriveKind, RawDrives, assemble};

fn bit(letter: char) -> u32 {
    1 << (u32::from(letter) - u32::from('A'))
}

#[test]
fn a_mapped_drive_windows_cannot_reach_is_disconnected() {
    let raw = RawDrives {
        mask: bit('Z'),
        kinds: vec![('Z', DriveKind::Network)],
        mapped: vec![('Z', r"\\nas\media".to_owned())],
        unavailable: vec!['z'],
        ..RawDrives::default()
    };
    let drives = assemble(&raw);
    assert_eq!(drives.len(), 1);
    assert!(!drives[0].connected);
    assert_eq!(drives[0].remote.as_deref(), Some(r"\\nas\media"));
    assert_eq!(drives[0].kind, DriveKind::Network);
}

#[test]
fn a_stick_on_a_remembered_letter_does_not_take_the_share() {
    let raw = RawDrives {
        mask: bit('Z'),
        kinds: vec![('Z', DriveKind::Removable)],
        remembered: vec![('Z', r"\\nas\share".to_owned())],
        ..RawDrives::default()
    };
    let drives = assemble(&raw);
    assert_eq!(drives[0].kind, DriveKind::Removable);
    assert_eq!(drives[0].remote, None);
    assert!(drives[0].connected);
}
