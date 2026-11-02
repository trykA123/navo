use std::time::Duration;

use super::{DriveKind, drive_details, list_drives};

#[cfg(windows)]
#[test]
fn the_system_drive_is_listed_as_fixed_with_space() {
    let drives = list_drives();
    let system = drives
        .iter()
        .find(|drive| drive.letter == Some('C'))
        .expect("C: is listed");
    assert_eq!(system.kind, DriveKind::Fixed);
    assert!(system.connected);
    let (_, space) = drive_details(&system.root, Duration::from_secs(5)).expect("C: answers");
    assert!(space.is_some_and(|space| space.total > 0 && space.free <= space.total));
}

#[cfg(unix)]
#[test]
fn the_slash_root_is_listed_first_with_space() {
    let drives = list_drives();
    assert_eq!(drives.first().map(|drive| drive.root.as_str()), Some("/"));
    assert_eq!(drives[0].kind, DriveKind::Fixed);
    let (_, space) = drive_details("/", Duration::from_secs(5)).expect("/ answers");
    assert!(space.is_some_and(|space| space.total > 0 && space.free <= space.total));
}
