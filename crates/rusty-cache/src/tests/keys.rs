use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_fs::FakeFs;

use crate::{CacheKeys, DriveIdentity};

fn local(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn probing(identity: DriveIdentity) -> CacheKeys {
    CacheKeys::new(Arc::new(move |_| Some(identity.clone())))
}

fn settled_key(keys: &CacheKeys, location: &Location) -> Option<u64> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(key) = keys.key(location) {
            return key.volume_serial;
        }
        assert!(Instant::now() < deadline, "the probe never answered");
        std::thread::yield_now();
    }
}

fn gated_probe() -> (CacheKeys, mpsc::Sender<()>) {
    let (release, gate) = mpsc::channel::<()>();
    let gate = Mutex::new(gate);
    let keys = CacheKeys::new(Arc::new(move |_| {
        let _ = gate.lock().unwrap().recv();
        Some(DriveIdentity::Serial(42))
    }));
    (keys, release)
}

#[test]
fn a_local_drive_uses_its_volume_serial() {
    let keys = probing(DriveIdentity::Serial(0x1234_abcd));
    let serial = settled_key(&keys, &local(r"C:\Users\admin"));
    assert_eq!(serial, Some(0x1234_abcd));
}

#[test]
fn a_mapped_drive_is_keyed_by_its_remote_name() {
    let first = probing(DriveIdentity::Remote(r"\\nas\media".to_owned()));
    let second = probing(DriveIdentity::Remote(r"\\nas\backup".to_owned()));
    let folder = local(r"Z:\films");
    let media = settled_key(&first, &folder);
    assert!(media.is_some());
    assert_ne!(media, settled_key(&second, &folder));
}

#[test]
fn a_unc_path_is_keyed_by_its_share_without_probing() {
    let keys = CacheKeys::new(Arc::new(|_| unreachable!("UNC paths need no probe")));
    let serial = |text: &str| keys.key(&local(text)).unwrap().volume_serial;
    let media = serial(r"\\NAS\Media\films");
    assert!(media.is_some());
    assert_eq!(media, serial(r"\\nas\media\shows"));
    assert_ne!(media, serial(r"\\nas\backup\films"));
}

#[test]
fn fake_locations_share_one_fixed_serial() {
    let fs = FakeFs::new();
    let keys = CacheKeys::default();
    let serial = |path: &str| keys.key(&fs.location(path)).unwrap().volume_serial;
    assert!(serial("home").is_some());
    assert_eq!(serial("home"), serial("srv/share/x"));
}

#[test]
fn an_unknown_drive_has_no_key_until_its_probe_answers() {
    let (keys, release) = gated_probe();
    let folder = local(r"D:\work");
    assert_eq!(keys.key(&folder), None);
    release.send(()).unwrap();
    assert_eq!(settled_key(&keys, &folder), Some(42));
}

#[test]
fn a_hung_probe_never_blocks_and_never_starts_a_second_thread() {
    let (keys, release) = gated_probe();
    let started = Instant::now();
    for _ in 0..5 {
        assert_eq!(keys.key(&local(r"E:\a")), None);
        assert_eq!(keys.key(&local(r"e:\b")), None);
    }
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(keys.probes_started(), 1);
    drop(release);
}

#[test]
fn an_unreadable_drive_is_probed_again_next_time() {
    let keys = CacheKeys::new(Arc::new(|_| None));
    let folder = local(r"F:\x");
    assert_eq!(keys.key(&folder), None);
    let deadline = Instant::now() + Duration::from_secs(5);
    while keys.probes_started() < 2 && Instant::now() < deadline {
        assert_eq!(keys.key(&folder), None);
        std::thread::yield_now();
    }
    assert!(keys.probes_started() >= 2);
}

fn settled_network(keys: &CacheKeys, location: &Location) -> bool {
    let _ = settled_key(keys, location);
    keys.is_network(location)
}

#[test]
fn a_unc_location_is_on_the_network() {
    assert!(CacheKeys::default().is_network(&local(r"\\srv\share\docs")));
}

#[test]
fn a_mapped_drive_is_on_the_network_once_probed() {
    let keys = probing(DriveIdentity::Remote(r"\\srv\share".to_owned()));
    assert!(settled_network(&keys, &local(r"Z:\docs")));
}

#[test]
fn a_local_drive_and_a_fake_location_are_not_on_the_network() {
    let keys = probing(DriveIdentity::Serial(7));
    assert!(!settled_network(&keys, &local(r"C:\docs")));
    assert!(!keys.is_network(&FakeFs::new().location("home")));
}
