//! The vault takes both Git pauses before its catalog transaction renames anything.

use super::*;

#[test]
fn capture_and_restore_pause_both_sides_before_the_rename() {
    let profile = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    fs.add_file("mnt/usb/docs/a", 7);
    let mut vault = open(profile.path(), &fs);
    let entry = capture(&mut vault, &fs.location("mnt/usb/docs"));
    assert_eq!(fs.pauses_before_rename(), vec![2]);

    let target = fs.location("mnt/usb/restored");
    restore(&mut vault, entry.id, &target).unwrap();
    assert_eq!(fs.pauses_before_rename(), vec![2, 4]);
}
