use std::cell::RefCell;
use std::time::{Duration, Instant};

use rusty_shell::{Drive, DriveKind, Space};
use rusty_ui::window_view::DriveIcon;

use super::DriveCache;

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
fn details_are_read_once_for_local_drives() {
    let probes = RefCell::new(Vec::new());
    let probe = |root: &str| {
        probes.borrow_mut().push(root.to_owned());
        Some((Some("System".to_owned()), Some(space(40))))
    };
    let mut cache = DriveCache::default();
    let start = Instant::now();
    let first = cache.refresh(vec![drive("C:\\", DriveKind::Fixed, true)], &probe, start);
    let later = start + Duration::from_secs(120);
    let again = cache.refresh(vec![drive("C:\\", DriveKind::Fixed, true)], &probe, later);
    assert_eq!(probes.borrow().len(), 1);
    assert_eq!(first, again);
    assert_eq!(first[0].label.as_deref(), Some("System"));
    assert_eq!(first[0].space, Some((100, 40)));
    assert_eq!(first[0].icon, DriveIcon::Fixed);
}

#[test]
fn network_drives_are_read_again_after_thirty_seconds() {
    let probes = RefCell::new(0);
    let probe = |_: &str| {
        *probes.borrow_mut() += 1;
        Some((None, Some(space(10))))
    };
    let mut cache = DriveCache::default();
    let start = Instant::now();
    let list = || vec![drive("Z:\\", DriveKind::Network, true)];
    cache.refresh(list(), &probe, start);
    cache.refresh(list(), &probe, start + Duration::from_secs(10));
    cache.refresh(list(), &probe, start + Duration::from_secs(31));
    assert_eq!(*probes.borrow(), 2);
}

#[test]
fn disconnected_drives_are_never_probed() {
    let probe = |_: &str| -> Option<super::Details> { panic!("no probe") };
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
    let answer = RefCell::new(Some((Some("Data".to_owned()), Some(space(5)))));
    let probe = |_: &str| answer.borrow_mut().take();
    let mut cache = DriveCache::default();
    let start = Instant::now();
    let list = || vec![drive("Z:\\", DriveKind::Network, true)];
    cache.refresh(list(), &probe, start);
    let later = cache.refresh(list(), &probe, start + Duration::from_secs(60));
    assert_eq!(later[0].label.as_deref(), Some("Data"));
}

#[test]
fn a_removed_drive_is_forgotten_and_probed_again_when_it_returns() {
    let probes = RefCell::new(0);
    let probe = |_: &str| {
        *probes.borrow_mut() += 1;
        Some((None, None))
    };
    let mut cache = DriveCache::default();
    let now = Instant::now();
    let stick = || vec![drive("E:\\", DriveKind::Removable, true)];
    cache.refresh(stick(), &probe, now);
    cache.refresh(Vec::new(), &probe, now);
    cache.refresh(stick(), &probe, now);
    assert_eq!(*probes.borrow(), 2);
}
