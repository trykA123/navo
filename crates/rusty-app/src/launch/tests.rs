use std::sync::mpsc;
use std::time::{Duration, Instant};

use rusty_core::Location;

use super::{command_for, queued};

#[test]
fn a_local_file_opens_with_the_system_handler() {
    let file = Location::new("/home/admin/notes.txt").unwrap();
    let command = command_for(&file).unwrap();
    let program = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    assert_eq!(command.get_program(), program);
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, ["/home/admin/notes.txt"]);
}

#[test]
fn a_location_from_another_provider_is_not_launched() {
    let remote = Location::new("fake://home/a.txt").unwrap();
    assert!(command_for(&remote).is_none());
}

#[test]
fn launching_returns_before_a_slow_start_finishes_and_keeps_order() {
    let (done, finished) = mpsc::channel();
    let launcher = queued(move |command| {
        std::thread::sleep(Duration::from_millis(150));
        let _ = done.send(command.get_args().next().map(ToOwned::to_owned));
    });
    let first = Location::new("/home/admin/a.txt").unwrap();
    let second = Location::new("/home/admin/b.txt").unwrap();
    let started = Instant::now();
    launcher(&first);
    launcher(&second);
    assert!(started.elapsed() < Duration::from_millis(100));
    let wait = Duration::from_secs(5);
    assert_eq!(
        finished.recv_timeout(wait).unwrap().unwrap(),
        "/home/admin/a.txt"
    );
    assert_eq!(
        finished.recv_timeout(wait).unwrap().unwrap(),
        "/home/admin/b.txt"
    );
}
