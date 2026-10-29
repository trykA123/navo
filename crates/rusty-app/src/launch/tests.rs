use std::sync::mpsc;
use std::time::{Duration, Instant};

use rusty_core::Location;

use super::{LaunchRequest, queued, request_for};

#[test]
fn a_local_file_launches_in_its_own_folder() {
    let file = Location::new("/home/admin/notes.txt").unwrap();
    assert_eq!(
        request_for(&file),
        Some(LaunchRequest {
            file: "/home/admin/notes.txt".to_owned(),
            directory: Some("/home/admin".to_owned()),
        })
    );
}

#[test]
fn a_windows_installer_launches_in_the_installer_folder() {
    let file = Location::new(r"C:\tools\setup\install.exe").unwrap();
    let request = request_for(&file).unwrap();
    assert_eq!(request.file, r"C:\tools\setup\install.exe");
    assert_eq!(request.directory.as_deref(), Some(r"C:\tools\setup"));
}

#[test]
fn a_location_without_a_parent_keeps_the_default_directory() {
    let root = Location::new("/").unwrap();
    let request = request_for(&root).unwrap();
    assert_eq!(request.directory, None);
}

#[test]
fn a_location_from_another_provider_is_not_launched() {
    let remote = Location::new("fake://home/a.txt").unwrap();
    assert!(request_for(&remote).is_none());
}

#[test]
fn launching_returns_before_a_slow_start_finishes_and_keeps_order() {
    let (done, finished) = mpsc::channel();
    let launcher = queued(move |request| {
        std::thread::sleep(Duration::from_millis(150));
        let _ = done.send(request.file);
    });
    let first = Location::new("/home/admin/a.txt").unwrap();
    let second = Location::new("/home/admin/b.txt").unwrap();
    let started = Instant::now();
    launcher(&first);
    launcher(&second);
    assert!(started.elapsed() < Duration::from_millis(100));
    let wait = Duration::from_secs(5);
    assert_eq!(finished.recv_timeout(wait).unwrap(), "/home/admin/a.txt");
    assert_eq!(finished.recv_timeout(wait).unwrap(), "/home/admin/b.txt");
}
