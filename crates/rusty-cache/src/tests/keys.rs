use std::sync::Arc;

use rusty_core::Location;
use rusty_fs::FakeFs;

use crate::{CacheKeys, DriveIdentity};

fn local(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn probing(identity: DriveIdentity) -> CacheKeys {
    CacheKeys::new(Arc::new(move |_| Some(identity.clone())))
}

#[test]
fn a_local_drive_uses_its_volume_serial() {
    let keys = probing(DriveIdentity::Serial(0x1234_abcd));
    let key = keys.key(&local(r"C:\Users\admin"));
    assert_eq!(key.volume_serial, Some(0x1234_abcd));
}

#[test]
fn a_mapped_drive_is_keyed_by_its_remote_name() {
    let first = probing(DriveIdentity::Remote(r"\\nas\media".to_owned()));
    let second = probing(DriveIdentity::Remote(r"\\nas\backup".to_owned()));
    let folder = local(r"Z:\films");
    assert!(first.key(&folder).volume_serial.is_some());
    assert_ne!(first.key(&folder), second.key(&folder));
}

#[test]
fn a_unc_path_is_keyed_by_its_share_without_probing() {
    let keys = CacheKeys::new(Arc::new(|_| unreachable!("UNC paths need no probe")));
    let media = keys.key(&local(r"\\NAS\Media\films")).volume_serial;
    let backup = keys.key(&local(r"\\nas\backup\films")).volume_serial;
    assert!(media.is_some());
    assert_eq!(media, keys.key(&local(r"\\nas\media\shows")).volume_serial);
    assert_ne!(media, backup);
}

#[test]
fn an_unreadable_drive_falls_back_to_its_root() {
    let keys = CacheKeys::default();
    let c = keys.key(&local(r"C:\a")).volume_serial;
    assert!(c.is_some());
    assert_eq!(c, keys.key(&local(r"c:\b")).volume_serial);
    assert_ne!(c, keys.key(&local(r"D:\a")).volume_serial);
}

#[test]
fn fake_locations_share_one_fixed_serial() {
    let fs = FakeFs::new();
    let keys = CacheKeys::default();
    let home = keys.key(&fs.location("home")).volume_serial;
    assert!(home.is_some());
    assert_eq!(home, keys.key(&fs.location("srv/share/x")).volume_serial);
    assert_eq!(
        home,
        CacheKeys::default().key(&fs.location("home")).volume_serial
    );
}
