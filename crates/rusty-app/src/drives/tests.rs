use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rusty_shell::{Drive, DriveKind, Space};
use rusty_ui::window_view::DriveIcon;

use super::{Details, DriveCache};

fn drive(root: &str, kind: DriveKind, connected: bool) -> Drive {
    Drive {
        root: root.to_owned(),
        letter: root.chars().next().filter(char::is_ascii_alphabetic),
        kind,
        label: None,
        remote: None,
        connected,
        space: None,
    }
}

fn space(free: u64) -> Space {
    Space { total: 100, free }
}

#[test]
fn local_drives_are_read_again_every_thirty_seconds() {
    let probes = AtomicUsize::new(0);
    let probe = |_: &str| {
        probes.fetch_add(1, Ordering::Relaxed);
        Some((Some("System".to_owned()), Some(space(40))))
    };
    let mut cache = DriveCache::default();
    let start = Instant::now();
    let list = || vec![drive("C:\\", DriveKind::Fixed, true)];
    let first = cache.refresh(list(), &probe, start);
    cache.refresh(list(), &probe, start + Duration::from_secs(10));
    cache.refresh(list(), &probe, start + Duration::from_secs(31));
    assert_eq!(probes.load(Ordering::Relaxed), 2);
    assert_eq!(first[0].label.as_deref(), Some("System"));
    assert_eq!(first[0].space, Some((100, 40)));
    assert_eq!(first[0].icon, DriveIcon::Fixed);
}

#[test]
fn removable_drives_are_read_on_every_poll() {
    let answers = Mutex::new(vec![
        (None, None),
        (Some("CARD".to_owned()), Some(space(7))),
    ]);
    let probe = |_: &str| answers.lock().unwrap().pop();
    let mut cache = DriveCache::default();
    let now = Instant::now();
    let reader = || vec![drive("E:\\", DriveKind::Removable, true)];
    let first = cache.refresh(reader(), &probe, now);
    let second = cache.refresh(reader(), &probe, now);
    assert_eq!(first[0].label.as_deref(), Some("CARD"));
    assert_eq!(second[0].label, None);
}

#[test]
fn a_letter_taken_by_another_device_is_read_again() {
    let probe = |_: &str| Some((Some("Share".to_owned()), None));
    let mut cache = DriveCache::default();
    let now = Instant::now();
    cache.refresh(vec![drive("Z:\\", DriveKind::Network, true)], &probe, now);
    let stick = drive("Z:\\", DriveKind::Removable, true);
    let shown = cache.refresh(
        vec![stick],
        &|_: &str| Some((Some("STICK".to_owned()), None)),
        now,
    );
    assert_eq!(shown[0].label.as_deref(), Some("STICK"));
}

#[test]
fn disconnected_drives_are_never_probed() {
    let probe = |_: &str| -> Option<Details> { panic!("no probe") };
    let mut cache = DriveCache::default();
    let shown = cache.refresh(
        vec![drive("Y:\\", DriveKind::Network, false)],
        &probe,
        Instant::now(),
    );
    assert!(!shown[0].connected);
    assert_eq!(shown[0].space, None);
}

#[test]
fn a_probe_that_times_out_keeps_the_last_details() {
    let answer = Mutex::new(Some((Some("Data".to_owned()), Some(space(5)))));
    let probe = |_: &str| answer.lock().unwrap().take();
    let mut cache = DriveCache::default();
    let start = Instant::now();
    let list = || vec![drive("Z:\\", DriveKind::Network, true)];
    cache.refresh(list(), &probe, start);
    let later = cache.refresh(list(), &probe, start + Duration::from_secs(60));
    assert_eq!(later[0].label.as_deref(), Some("Data"));
}

#[test]
fn slow_probes_run_together() {
    let probe = |_: &str| {
        std::thread::sleep(Duration::from_millis(300));
        Some((None, None))
    };
    let mut cache = DriveCache::default();
    let drives = ["X:\\", "Y:\\", "Z:\\", "W:\\"]
        .map(|root| drive(root, DriveKind::Network, true))
        .to_vec();
    let started = Instant::now();
    cache.refresh(drives, &probe, started);
    assert!(started.elapsed() < Duration::from_millis(900));
}
